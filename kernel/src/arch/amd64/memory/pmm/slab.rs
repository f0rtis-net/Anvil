#![allow(dead_code)]

use core::{
    mem,
    ptr::{self, NonNull},
};

use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

use crate::{
    arch::amd64::memory::{
        misc::{align_up, phys_to_virt, virt_to_phys},
        pmm::{
            pages_allocator::{KERNEL_PAGES, alloc_pages_by_order, free_pages},
            sparsemem::{get_sparse_memory, INVALID_PFN, PAGE_SHIFT, PAGE_SIZE},
            zones_manager::ZoneId,
        },
    },
    early_println,
};

const SLAB_MAGIC:        u32   = 0xC0FF_EE42;
const SLAB_POISON:       u8    = 0xDE;
const MAX_SLAB_ORDER:    usize = 4;
const MIN_OBJS_PER_SLAB: usize = 8;
const MAX_EMPTY_SLABS:   usize = 2;

pub const SLAB_MIN_ALIGN: usize = 16;
pub const SLAB_MAX_ALLOC: usize = 2048;

const CLASSES: &[usize] = &[
    8, 16, 32, 64, 96, 128, 192, 256, 384, 512, 768, 1024, 1536, 2048,
];

#[repr(C)]
struct FreeNode {
    next:   *mut FreeNode,
    poison: usize,
}

const POISON_TAG: usize = 0xDEAD_BEEF_DEAD_BEEF;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum SlabList {
    Partial = 0,
    Full    = 1,
    Empty   = 2,
}

#[repr(C)]
struct SlabHeader {
    magic:     u32,
    class_idx: u16,
    order:     u8,
    list:      SlabList,

    inuse:     u16,
    total:     u16,

    free:      *mut FreeNode,
    prev:      *mut SlabHeader,
    next:      *mut SlabHeader,
}

impl SlabHeader {
    #[inline]
    fn span_bytes(&self) -> usize {
        PAGE_SIZE << (self.order as usize)
    }

    #[inline]
    fn as_ptr(&mut self) -> *mut SlabHeader {
        self as *mut _
    }

    #[inline]
    fn base_virt(&self) -> usize {
        self as *const _ as usize
    }

    #[inline]
    fn objects_start(&self, obj_size: usize) -> usize {
        align_up(self.base_virt() + mem::size_of::<SlabHeader>(), obj_size)
    }

    #[inline]
    fn is_full(&self) -> bool {
        self.inuse as usize == self.total as usize
    }

    #[inline]
    fn is_empty_slab(&self) -> bool {
        self.inuse == 0
    }
}

struct SlabBucket {
    head:  *mut SlabHeader,
    count: usize,
}

impl SlabBucket {
    const fn empty() -> Self {
        Self { head: ptr::null_mut(), count: 0 }
    }

    #[inline]
    fn is_empty(&self) -> bool { self.head.is_null() }

    #[inline]
    fn len(&self) -> usize { self.count }

    fn push(&mut self, slab: *mut SlabHeader, tag: SlabList) {
        unsafe {
            (*slab).list = tag;
            (*slab).prev = ptr::null_mut();
            (*slab).next = self.head;
            if !self.head.is_null() {
                (*self.head).prev = slab;
            }
            self.head = slab;
        }
        self.count += 1;
    }

    fn remove(&mut self, slab: *mut SlabHeader) {
        debug_assert!(self.count > 0, "SlabBucket::remove: underflow");
        unsafe {
            let prev = (*slab).prev;
            let next = (*slab).next;

            if !prev.is_null() { (*prev).next = next; } else { self.head = next; }
            if !next.is_null() { (*next).prev = prev; }

            (*slab).prev = ptr::null_mut();
            (*slab).next = ptr::null_mut();
        }
        self.count -= 1;
    }

    fn pop(&mut self) -> Option<*mut SlabHeader> {
        if self.head.is_null() { return None; }
        let slab = self.head;
        self.remove(slab);
        Some(slab)
    }
}

#[derive(Clone, Copy)]
enum BucketKind { Partial, Full, Empty }

struct Cache {
    obj_size:      usize,
    order:         usize,
    objs_per_slab: usize,

    partial: SlabBucket,
    full:    SlabBucket,
    empty:   SlabBucket,
}

impl Cache {
    const fn zeroed() -> Self {
        Self {
            obj_size:      0,
            order:         0,
            objs_per_slab: 0,
            partial:       SlabBucket::empty(),
            full:          SlabBucket::empty(),
            empty:         SlabBucket::empty(),
        }
    }

    fn init(&mut self, size_class: usize) {
        let obj_size  = size_class.max(mem::size_of::<FreeNode>());
        self.obj_size = align_up(obj_size, SLAB_MIN_ALIGN);

        let mut order = 0usize;
        loop {
            let span   = PAGE_SIZE << order;
            let base   = align_up(mem::size_of::<SlabHeader>(), self.obj_size);
            let usable = span.saturating_sub(base);
            let n      = usable / self.obj_size;

            if n >= MIN_OBJS_PER_SLAB || order >= MAX_SLAB_ORDER {
                self.order         = order;
                self.objs_per_slab = n.max(1);
                break;
            }
            order += 1;
        }
    }

    fn bucket_mut(&mut self, kind: BucketKind) -> &mut SlabBucket {
        match kind {
            BucketKind::Partial => &mut self.partial,
            BucketKind::Full    => &mut self.full,
            BucketKind::Empty   => &mut self.empty,
        }
    }

    fn move_slab(&mut self, slab: *mut SlabHeader, from: BucketKind, to: BucketKind, tag: SlabList) {
        self.bucket_mut(from).remove(slab);
        self.bucket_mut(to).push(slab, tag);
    }
}

pub struct SlabAllocator {
    zone:   ZoneId,
    caches: [Cache; CLASSES.len()],
}

unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}

impl SlabAllocator {
    pub const fn new(zone: ZoneId) -> Self {
        const EMPTY_CACHE: Cache = Cache::zeroed();
        Self { zone, caches: [EMPTY_CACHE; CLASSES.len()] }
    }

    pub fn init(&mut self) {
        for (i, cache) in self.caches.iter_mut().enumerate() {
            cache.init(CLASSES[i]);
        }
    }

    #[inline]
    fn class_index(size: usize) -> Option<usize> {
        let needed = size.max(mem::size_of::<FreeNode>());
        CLASSES.iter().position(|&cls| needed <= cls)
    }

    pub fn alloc(&mut self, size: usize, zeroed: bool) -> Option<NonNull<u8>> {
        let idx  = Self::class_index(size)?;
        let slab = self.get_or_grow_partial(idx)?;

        unsafe {
            let sh = &mut *slab;

            debug_assert_eq!(sh.list, SlabList::Partial, "get_or_grow_partial must return Partial slab");
            debug_assert!(!sh.free.is_null(), "partial slab has null freelist");

            let node  = sh.free;
            sh.free   = (*node).next;
            sh.inuse += 1;

            (*node).poison = 0;

            if sh.is_full() {
                self.caches[idx].move_slab(slab, BucketKind::Partial, BucketKind::Full, SlabList::Full);
            }

            let obj = node as *mut u8;
            if zeroed {
                ptr::write_bytes(obj, 0, self.caches[idx].obj_size);
            }

            Some(NonNull::new_unchecked(obj))
        }
    }

    fn get_or_grow_partial(&mut self, idx: usize) -> Option<*mut SlabHeader> {
        if !self.caches[idx].partial.is_empty() {
            return Some(self.caches[idx].partial.head);
        }

        if let Some(slab) = self.caches[idx].empty.pop() {
            let obj_size      = self.caches[idx].obj_size;
            let objs_per_slab = self.caches[idx].objs_per_slab;
            unsafe {
                (*slab).free  = Self::build_freelist((*slab).base_virt(), obj_size, objs_per_slab);
                (*slab).inuse = 0;
            }
            self.caches[idx].partial.push(slab, SlabList::Partial);
            return Some(slab);
        }

        let slab = self.grow(idx)?;
        self.caches[idx].partial.push(slab, SlabList::Partial);
        Some(slab)
    }

    pub fn free(&mut self, p: NonNull<u8>) -> bool {
        let ptr_u = p.as_ptr() as usize;

        let slab = match self.find_slab(ptr_u) {
            Some(s) => s,
            None    => return false,
        };

        unsafe {
            let sh  = &mut *slab;
            let idx = sh.class_idx as usize;

            if idx >= self.caches.len() { return false; }

            let obj_size = self.caches[idx].obj_size;
            let obj_base = sh.objects_start(obj_size);
            let slab_end = sh.base_virt() + sh.span_bytes();

            if ptr_u < obj_base || ptr_u >= slab_end      { return false; }
            if (ptr_u - obj_base) % obj_size != 0         { return false; }
            if sh.inuse == 0                               { return false; }

            let node = ptr_u as *mut FreeNode;

            #[cfg(debug_assertions)]
            if (*node).poison == POISON_TAG {
                panic!(
                    "slab: double-free at {:#x} class_idx={} order={}",
                    ptr_u, idx, sh.order
                );
            }

            #[cfg(debug_assertions)]
            ptr::write_bytes(ptr_u as *mut u8, SLAB_POISON, obj_size);

            (*node).next   = sh.free;
            (*node).poison = POISON_TAG;
            sh.free        = node;

            let was_full = sh.list == SlabList::Full;
            sh.inuse    -= 1;

            if was_full {
                self.caches[idx].move_slab(slab, BucketKind::Full, BucketKind::Partial, SlabList::Partial);
            }

            if sh.is_empty_slab() {
                self.caches[idx].move_slab(slab, BucketKind::Partial, BucketKind::Empty, SlabList::Empty);

                if self.caches[idx].empty.len() > MAX_EMPTY_SLABS {
                    if let Some(victim) = self.caches[idx].empty.pop() {
                        Self::release_slab(victim);
                    }
                }
            }
        }

        true
    }

    fn grow(&mut self, class_idx: usize) -> Option<*mut SlabHeader> {
        let order         = self.caches[class_idx].order;
        let obj_size      = self.caches[class_idx].obj_size;
        let objs_per_slab = self.caches[class_idx].objs_per_slab;

        let phys = alloc_pages_by_order(order, KERNEL_PAGES)?;
        let virt = phys_to_virt(phys.as_u64() as usize);
        let slab = virt as *mut SlabHeader;

        unsafe {
            (*slab).magic     = SLAB_MAGIC;
            (*slab).class_idx = class_idx as u16;
            (*slab).order     = order as u8;
            (*slab).list      = SlabList::Partial;
            (*slab).inuse     = 0;
            (*slab).total     = objs_per_slab as u16;
            (*slab).free      = Self::build_freelist(virt, obj_size, objs_per_slab);
            (*slab).prev      = ptr::null_mut();
            (*slab).next      = ptr::null_mut();
        }

        Self::register_slab_in_sparsemem(slab, order);
        Some(slab)
    }

    fn build_freelist(slab_virt: usize, obj_size: usize, count: usize) -> *mut FreeNode {
        let base = align_up(slab_virt + mem::size_of::<SlabHeader>(), obj_size);
        let mut cur: *mut FreeNode = ptr::null_mut();

        for i in (0..count).rev() {
            let node = (base + i * obj_size) as *mut FreeNode;
            unsafe {
                (*node).next   = cur;
                (*node).poison = POISON_TAG;
            }
            cur = node;
        }

        cur
    }

    fn register_slab_in_sparsemem(slab: *mut SlabHeader, order: usize) {
        let sparse   = get_sparse_memory();
        let base_virt= unsafe { (*slab).base_virt() };
        let head_pfn = virt_to_phys(base_virt) >> PAGE_SHIFT;
        let pages    = 1usize << order;

        for i in 0..pages {
            if let Some(f) = sparse.pfn_to_frame(head_pfn + i) {
                f.next_free = head_pfn;
            }
        }
    }

    fn release_slab(slab: *mut SlabHeader) {
        unsafe {
            let order     = (*slab).order as usize;
            let base_virt = (*slab).base_virt();
            let base_phys = virt_to_phys(base_virt);
            let head_pfn  = base_phys >> PAGE_SHIFT;
            let pages     = 1usize << order;
            let sparse    = get_sparse_memory();

            for i in 0..pages {
                if let Some(f) = sparse.pfn_to_frame(head_pfn + i) {
                    f.next_free = INVALID_PFN;
                }
            }

            (*slab).magic = 0;
            free_pages(PhysAddr::new(base_phys as u64));
        }
    }

    fn find_slab(&self, ptr_u: usize) -> Option<*mut SlabHeader> {
        let sparse   = get_sparse_memory();
        let pfn      = virt_to_phys(ptr_u) >> PAGE_SHIFT;
        let frame    = sparse.pfn_to_frame(pfn)?;

        let head_pfn = frame.next_free;
        if head_pfn == INVALID_PFN { return None; }

        let slab = phys_to_virt(head_pfn << PAGE_SHIFT) as *mut SlabHeader;
        unsafe {
            if (*slab).magic != SLAB_MAGIC { return None; }
        }
        Some(slab)
    }
}


static SLAB: Mutex<SlabAllocator> = Mutex::new(SlabAllocator::new(ZoneId::High));

pub fn slab_init() {
    early_println!("Initializing slab allocator...");
    SLAB.lock().init();
    early_println!("Slab allocator initialized!");
}

pub fn slab_alloc(size: usize, zeroed: bool) -> Option<VirtAddr> {
    SLAB.lock()
        .alloc(size, zeroed)
        .map(|p| VirtAddr::new(p.as_ptr() as u64))
}

pub fn slab_free(ptr: VirtAddr) -> bool {
    if ptr.as_u64() == 0 {
        return false;
    }
    let nn = unsafe { NonNull::new_unchecked(ptr.as_u64() as *mut u8) };
    SLAB.lock().free(nn)
}