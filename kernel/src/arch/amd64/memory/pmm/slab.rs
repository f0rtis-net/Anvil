use core::{
    mem,
    ptr::{self, NonNull},
};

use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr};

use crate::{arch::amd64::memory::{misc::{align_up, phys_to_virt, virt_to_phys}, pmm::{HHDM_OFFSET, pages_allocator::{KERNEL_PAGES, SAFE_KERNEL_PAGES, alloc_pages_by_order, free_pages}, sparsemem::PAGE_SIZE, zones_manager::ZoneId}}, serial_println};

const SLAB_MAGIC: u32 = 0xC0FF_EE42;

const MAX_SLAB_ORDER: usize = 4;
const MIN_OBJS_PER_SLAB: usize = 8;

pub const SLAB_MAX_ALLOC: usize = 2048;

const CLASSES: &[usize] = &[
    8, 16, 32, 64, 128, 256, 512, 1024, 2048,
];

#[repr(u8)]
#[derive(Copy, Clone, Eq, PartialEq)]
enum SlabList {
    Partial = 1,
    Full    = 2,
    Empty   = 3,
}

#[repr(C)]
struct FreeNode {
    next: *mut FreeNode,
}

#[repr(C)]
struct SlabHeader {
    magic: u32,
    class_idx: u16,
    order: u8,
    list: SlabList, 

    inuse: u16,
    total: u16,

    free: *mut FreeNode,

    prev: *mut SlabHeader,
    next: *mut SlabHeader,
}

impl SlabHeader {
    #[inline]
    fn span_bytes(&self) -> usize {
        PAGE_SIZE << (self.order as usize)
    }

    #[inline]
    fn base(&self) -> usize {
        self as *const _ as usize
    }
}

struct Cache {
    obj_size: usize,
    order: usize,
    objs_per_slab: usize,

    partial: *mut SlabHeader,
    full: *mut SlabHeader,
    empty: *mut SlabHeader,
}

impl Cache {
    const fn empty() -> Self {
        Self {
            obj_size: 0,
            order: 0,
            objs_per_slab: 0,
            partial: ptr::null_mut(),
            full: ptr::null_mut(),
            empty: ptr::null_mut(),
        }
    }
}

#[inline]
fn list_push(head: &mut *mut SlabHeader, slab: *mut SlabHeader) {
    unsafe {
        (*slab).prev = ptr::null_mut();
        (*slab).next = *head;

        if !(*head).is_null() {
            (**head).prev = slab;
        }

        *head = slab;
    }
}

#[inline]
fn list_remove(head: &mut *mut SlabHeader, slab: *mut SlabHeader) {
    unsafe {
        let prev = (*slab).prev;
        let next = (*slab).next;

        if !prev.is_null() {
            (*prev).next = next;
        } else {
            *head = next;
        }

        if !next.is_null() {
            (*next).prev = prev;
        }

        (*slab).prev = ptr::null_mut();
        (*slab).next = ptr::null_mut();
    }
}

#[inline]
fn count_list(mut head: *mut SlabHeader) -> usize {
    let mut n = 0;
    while !head.is_null() {
        n += 1;
        unsafe { head = (*head).next; }
    }
    n
}

pub struct SlabAllocator {
    zone: ZoneId,
    caches: [Cache; CLASSES.len()],
}

unsafe impl Send for SlabAllocator {}
unsafe impl Sync for SlabAllocator {}


impl SlabAllocator {
    pub const fn new(zone: ZoneId) -> Self {
        const EMPTY: Cache = Cache::empty();
        Self {
            zone,
            caches: [EMPTY; CLASSES.len()],
        }
    }

    pub fn init(&mut self) {
        for (i, c) in self.caches.iter_mut().enumerate() {
            let size = CLASSES[i].max(mem::size_of::<FreeNode>());
            c.obj_size = align_up(size, mem::align_of::<usize>());

            let mut order = 0usize;
            loop {
                let span = PAGE_SIZE << order;
                let usable = span.saturating_sub(mem::size_of::<SlabHeader>());
                let n = usable / c.obj_size;

                if n >= MIN_OBJS_PER_SLAB || order >= MAX_SLAB_ORDER {
                    c.order = order;
                    c.objs_per_slab = n.max(1);
                    break;
                }
                order += 1;
            }

            c.partial = ptr::null_mut();
            c.full = ptr::null_mut();
            c.empty = ptr::null_mut();
        }
    }

    #[inline]
    fn class_index(size: usize) -> Option<usize> {
        let s = size.max(mem::size_of::<FreeNode>());
        for (i, &cls) in CLASSES.iter().enumerate() {
            if s <= cls {
                return Some(i);
            }
        }
        None
    }

    pub fn alloc(&mut self, size: usize, zeroed: bool) -> Option<NonNull<u8>> {
        let idx = Self::class_index(size)?;
        let cache: *mut Cache = &mut self.caches[idx];

        unsafe {
            if (*cache).partial.is_null() {
                if !(*cache).empty.is_null() {
                    // empty → partial
                    let slab = (*cache).empty;
                    list_remove(&mut (*cache).empty, slab);

                    (*slab).list = SlabList::Partial;
                    list_push(&mut (*cache).partial, slab);
                } else {
                    // grow → partial
                    let slab = self.grow(idx, zeroed)?;
                    (*slab).list = SlabList::Partial;
                    list_push(&mut (*cache).partial, slab);
                }
            }

            let slab = &mut *(*cache).partial;
            debug_assert!(slab.list == SlabList::Partial);
            debug_assert!(!slab.free.is_null());

            // pop freelist
            let node = slab.free;
            slab.free = (*node).next;
            slab.inuse += 1;

            if slab.inuse as usize == slab.total as usize {
                let slab_ptr = slab as *mut SlabHeader;
                list_remove(&mut (*cache).partial, slab_ptr);

                slab.list = SlabList::Full;
                list_push(&mut (*cache).full, slab_ptr);
            }

            let obj = node as *mut u8;

            if zeroed {
                ptr::write_bytes(obj, 0, (*cache).obj_size);
            }

            Some(NonNull::new_unchecked(obj))
        }
    }

    pub fn free(&mut self, p: NonNull<u8>) -> bool {
        let ptr_u = p.as_ptr() as usize;

        let (slab_base, order) = match self.find_slab_base(ptr_u) {
            Some(v) => v,
            None => return false, 
        };

        let slab = unsafe { &mut *(slab_base as *mut SlabHeader) };

        if slab.magic != SLAB_MAGIC {
            return false;
        }

        let idx = slab.class_idx as usize;
        if idx >= self.caches.len() || slab.order as usize != order {
            return false;
        }

        let cache: *mut Cache = &mut self.caches[idx];

        unsafe {
            let slab_ptr = slab as *mut SlabHeader;

            let node = p.as_ptr() as *mut FreeNode;
            (*node).next = slab.free;
            slab.free = node;

            let was_full = slab.list == SlabList::Full;
            slab.inuse -= 1;

            // 3. FULL → PARTIAL
            if was_full {
                list_remove(&mut (*cache).full, slab_ptr);

                slab.list = SlabList::Partial;
                list_push(&mut (*cache).partial, slab_ptr);
            }

            if slab.inuse == 0 {
                match slab.list {
                    SlabList::Partial => {
                        list_remove(&mut (*cache).partial, slab_ptr);
                    }
                    SlabList::Full => {
                        list_remove(&mut (*cache).full, slab_ptr);
                    }
                    SlabList::Empty => {
                    }
                }

                slab.list = SlabList::Empty;
                list_push(&mut (*cache).empty, slab_ptr);

                if count_list((*cache).empty) > 2 {
                    let victim = (*cache).empty;
                    list_remove(&mut (*cache).empty, victim);

                    (*victim).magic = 0;
                    let phys = virt_to_phys(HHDM_OFFSET, victim as usize);

                    free_pages(PhysAddr::new(phys as u64));
                }
            }

            true
        }
    }   

    fn grow(&mut self, class_idx: usize, zeroed: bool) -> Option<*mut SlabHeader> {
        let cache = &self.caches[class_idx];

        let zero_flag = if zeroed { SAFE_KERNEL_PAGES } else { KERNEL_PAGES };

        // phys
        let phys = alloc_pages_by_order(cache.order, zero_flag)?;
        let phys_u = phys.as_u64() as usize;

        // virt (HHDM)
        let span = phys_to_virt(unsafe { HHDM_OFFSET }, phys_u);

        let slab = span as *mut SlabHeader;

        unsafe {
            (*slab).magic = SLAB_MAGIC;
            (*slab).class_idx = class_idx as u16;
            (*slab).order = cache.order as u8;
            (*slab).list = SlabList::Empty;

            (*slab).inuse = 0;
            (*slab).total = cache.objs_per_slab as u16;

            (*slab).free = ptr::null_mut();
            (*slab).prev = ptr::null_mut();
            (*slab).next = ptr::null_mut();

            let base = span + mem::size_of::<SlabHeader>();
            let mut head: *mut FreeNode = ptr::null_mut();

            for i in 0..cache.objs_per_slab {
                let obj = (base + i * cache.obj_size) as *mut FreeNode;
                (*obj).next = head;
                head = obj;
            }

            (*slab).free = head;
        }

        Some(slab)
    }

    fn find_slab_base(&self, ptr_u: usize) -> Option<(usize, usize)> {
        for order in 0..=MAX_SLAB_ORDER {
            let span = PAGE_SIZE << order;
            let base = ptr_u & !(span - 1);

            let hdr = base as *const SlabHeader;
            unsafe {
                if (*hdr).magic == SLAB_MAGIC && (*hdr).order as usize == order {
                    return Some((base, order));
                }
            }
        }
        None
    }
}

static SLAB: Mutex<SlabAllocator> = Mutex::new(SlabAllocator::new(ZoneId::High));

pub fn slab_init() {
    serial_println!("Initializing slab allocator...");
    SLAB.lock().init();
    serial_println!("Slab allocator initialized!");
}

pub fn slab_alloc(size: usize, zeroed: bool) -> Option<VirtAddr> {
    SLAB
        .lock()
        .alloc(size, zeroed)
        .map(|p| VirtAddr::new(p.as_ptr() as u64))
}

pub fn slab_try_free(ptr: VirtAddr) -> bool {
    if ptr.as_u64() == 0 {
        return false;
    }

    let nn = unsafe {
        NonNull::new_unchecked(ptr.as_u64() as usize as *mut u8)
    };

    SLAB.lock().free(nn)
}


