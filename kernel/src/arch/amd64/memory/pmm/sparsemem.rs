#![allow(dead_code)]

use core::ptr::null_mut;

use spin::Once;

use crate::{
    arch::amd64::memory::{
        misc::{align_up, phys_to_virt},
        pmm::{
            buddy::BuddyTag,
            bump_alloc::BumpState,
            memblock::{Memblock, MemblockError, MemblockType},
            zones_manager::ZoneId,
        },
    },
    early_println,
};

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

pub const SECTION_SHIFT: usize = 27; 
pub const SECTION_SIZE: usize = 1 << SECTION_SHIFT;

pub const PAGES_PER_SECTION: usize = 1 << (SECTION_SHIFT - PAGE_SHIFT);

const SECTION_PFN_SHIFT: usize = SECTION_SHIFT - PAGE_SHIFT;

pub type Pfn = usize;
pub const INVALID_PFN: Pfn = Pfn::MAX;


#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameState {
    Absent   = 0, 
    Usable   = 1, 
    Reserved = 2, 
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Frame {
    pub state:     FrameState,
    pub order:     u8,
    pub tag:       BuddyTag,
    pub zone:      ZoneId,
    pub next_free: Pfn,
    pub prev_free: Pfn,
}

impl Frame {
    #[inline]
    pub const fn absent() -> Self {
        Self {
            state:     FrameState::Absent,
            order:     0,
            tag:       BuddyTag::Unused,
            zone:      ZoneId::Normal,
            next_free: INVALID_PFN,
            prev_free: INVALID_PFN,
        }
    }
}

#[repr(C)]
pub struct Memsection {
    pub frames:  *mut Frame,
    pub present: bool,
}

impl Memsection {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            frames:  null_mut(),
            present: false,
        }
    }
}

static SPARSE_MEMORY: Once<SparseMem> = Once::new();

#[inline]
pub fn get_sparse_memory() -> &'static SparseMem {
    SPARSE_MEMORY.get().expect("Sparsemem not initialized")
}

pub struct SparseMem {
    pub sections:        *mut Memsection,
    pub section_count:   usize,
    pub max_present_sec: usize,

    cached_max_pfn: Pfn,
}

unsafe impl Send for SparseMem {}
unsafe impl Sync for SparseMem {}

impl SparseMem {
    pub const fn empty() -> Self {
        Self {
            sections:        null_mut(),
            section_count:   0,
            max_present_sec: 0,
            cached_max_pfn:  0,
        }
    }

    #[inline]
    pub fn is_initialized(&self) -> bool {
        !self.sections.is_null()
    }

    #[inline]
    pub fn pfn_to_section(pfn: Pfn) -> usize {
        pfn >> SECTION_PFN_SHIFT
    }

    #[inline]
    fn pfn_to_offset(pfn: Pfn) -> usize {
        pfn & (PAGES_PER_SECTION - 1)
    }

    #[inline]
    fn section_base_pfn(sec: usize) -> Pfn {
        sec << SECTION_PFN_SHIFT
    }

    #[inline]
    fn sections_mut(&mut self) -> &mut [Memsection] {
        unsafe { core::slice::from_raw_parts_mut(self.sections, self.section_count) }
    }

    #[inline]
    pub fn sections(&self) -> &[Memsection] {
        unsafe { core::slice::from_raw_parts(self.sections, self.section_count) }
    }

    #[inline]
    pub fn pfn_present(&self, pfn: Pfn) -> bool {
        let sec = Self::pfn_to_section(pfn);
        sec < self.section_count && self.sections()[sec].present
    }

    #[inline]
    pub fn max_present_pfn(&self) -> Pfn {
        self.cached_max_pfn
    }

    /// Returns a raw pointer to the Frame for `pfn`.
    ///
    /// # Safety
    /// The caller must ensure no two live `*mut Frame` pointers to the same PFN
    /// are dereferenced concurrently (i.e. the PMM lock must be held).
    #[inline]
    pub fn pfn_to_frame(&self, pfn: Pfn) -> Option<*mut Frame> {
        if !self.pfn_present(pfn) {
            return None;
        }
        let sec = Self::pfn_to_section(pfn);
        let off = Self::pfn_to_offset(pfn);

        unsafe {
            let section = &*self.sections.add(sec);
            Some(section.frames.add(off))
        }
    }

    pub fn init_from_memblock(&mut self, bump: &mut BumpState, memblock: &Memblock) {
        debug_assert!(!self.is_initialized());

        let max_phys = memblock.max_phys_addr() as usize;
        if max_phys == 0 {
            return;
        }

        let section_count = section_count_from_max_phys(max_phys);

        let bytes = section_count * core::mem::size_of::<Memsection>();
        let align = core::mem::align_of::<Memsection>();

        let ptr = bump
            .alloc_zeroed(bytes, align)
            .expect("OOM: sparsemem section table")
            .as_ptr() as *mut Memsection;

        for i in 0..section_count {
            unsafe { ptr.add(i).write(Memsection::empty()); }
        }

        self.sections      = ptr;
        self.section_count = section_count;

        for r in memblock.memory_regions() {
            if r.kind != MemblockType::Usable || r.size == 0 {
                continue;
            }
            self.mark_range(bump, r.base as usize, r.end() as usize, FrameState::Usable, true);
        }

        for r in memblock.reserved_regions() {
            if r.size == 0 {
                continue;
            }
            self.mark_range(bump, r.base as usize, r.end() as usize, FrameState::Reserved, false);
        }

        self.cached_max_pfn = self.compute_max_present_pfn();
    }

    fn compute_max_present_pfn(&self) -> Pfn {
        let mut max: Pfn = 0;

        for sec in 0..self.section_count {
            let s = unsafe { &*self.sections.add(sec) };
            if !s.present {
                continue;
            }

            let base = Self::section_base_pfn(sec);

            for i in (0..PAGES_PER_SECTION).rev() {
                let f = unsafe { &*s.frames.add(i) };
                if f.state != FrameState::Absent {
                    let pfn = base + i + 1;
                    if pfn > max {
                        max = pfn;
                    }
                    break; 
                }
            }
        }

        max
    }

    fn ensure_section(&mut self, bump: &mut BumpState, sec: usize) {
        if sec >= self.section_count {
            return;
        }

        if unsafe { (*self.sections.add(sec)).present } {
            return;
        }

        let bytes = PAGES_PER_SECTION * core::mem::size_of::<Frame>();
        let align = core::mem::align_of::<Frame>();

        let frames = bump
            .alloc_zeroed(bytes, align)
            .expect("OOM: sparsemem frames")
            .as_ptr() as *mut Frame;

        for i in 0..PAGES_PER_SECTION {
            unsafe { frames.add(i).write(Frame::absent()); }
        }

        unsafe {
            let s = &mut *self.sections.add(sec);
            s.frames  = frames;
            s.present = true;
        }

        if sec > self.max_present_sec {
            self.max_present_sec = sec;
        }
    }

    fn mark_range(
        &mut self,
        bump: &mut BumpState,
        base: usize,
        end: usize,
        state: FrameState,
        create_sections: bool,
    ) {
        if base >= end {
            return;
        }

        let start_pfn = base >> PAGE_SHIFT;
        let end_pfn = (end + PAGE_SIZE - 1) >> PAGE_SHIFT;

        let mut pfn = start_pfn;

        while pfn < end_pfn {
            let sec = Self::pfn_to_section(pfn);
            if sec >= self.section_count {
                break;
            }

            if create_sections {
                self.ensure_section(bump, sec);
            }

            if !unsafe { (*self.sections.add(sec)).present } {
                pfn = core::cmp::min(end_pfn, Self::section_base_pfn(sec + 1));
                continue;
            }

            let sec_base = Self::section_base_pfn(sec);
            let chunk_end = core::cmp::min(end_pfn, sec_base + PAGES_PER_SECTION);
            let off = pfn - sec_base;
            let len = chunk_end - pfn;

            unsafe {
                let frames_ptr = (*self.sections.add(sec)).frames.add(off);
                let slice = core::slice::from_raw_parts_mut(frames_ptr, len);
                for f in slice {
                    f.state = state;
                }
            }

            pfn = chunk_end;
        }
    }
}


#[inline]
fn section_count_from_max_phys(max_phys: usize) -> usize {
    if max_phys == 0 {
        0
    } else {
        ((max_phys - 1) >> SECTION_SHIFT) + 1
    }
}

fn count_present_sections(memblock: &Memblock) -> usize {
    const MAX_SECTIONS: usize = (512 * 1024 * 1024 * 1024usize) / SECTION_SIZE;
    // 512 бит → 64 u64
    const WORDS: usize = MAX_SECTIONS / 64;

    let mut seen = [0u64; WORDS];

    let mut set_bit = |sec: usize| {
        if sec < MAX_SECTIONS {
            seen[sec / 64] |= 1u64 << (sec % 64);
        }
    };

    for r in memblock.memory_regions() {
        if r.size == 0 {
            continue;
        }
        let start_sec = (r.base as usize) >> SECTION_SHIFT;
        let end_sec   = (r.end() as usize).saturating_sub(1) >> SECTION_SHIFT;
        for sec in start_sec..=end_sec {
            set_bit(sec);
        }
    }

    seen.iter().map(|w| w.count_ones() as usize).sum()
}

fn sparsemem_required_bytes(memblock: &Memblock) -> usize {
    let max_phys         = memblock.max_phys_addr() as usize;
    let section_count    = section_count_from_max_phys(max_phys);
    let present_sections = count_present_sections(memblock);

    let align_slack = core::mem::align_of::<Frame>() * (present_sections + 1);

    section_count    * core::mem::size_of::<Memsection>()
        + present_sections * PAGES_PER_SECTION * core::mem::size_of::<Frame>()
        + align_slack
}

fn find_suitable_region(memblock: &Memblock, need_bytes: usize) -> Option<(u64, u64)> {
    for r in memblock.memory_regions() {
        if r.size == 0 {
            continue;
        }

        let aligned_base = align_up(r.base as usize, PAGE_SIZE);
        let region_end   = (r.base + r.size) as usize;

        if aligned_base >= region_end {
            continue;
        }

        let available = region_end - aligned_base;
        if available >= need_bytes {
            return Some((r.base, r.size));
        }
    }

    None
}

fn reserve_for_sparsemem(
    memblock: &mut Memblock,
    base: u64,
    size: u64,
) -> Result<(), MemblockError> {
    memblock.add_reserved(base, size, MemblockType::Reserved)?;
    memblock.normalize()?;
    Ok(())
}

pub fn init_sparsemem_layer(memblock: &mut Memblock) {
    early_println!("Initializing sparse memory model...");

    let need_bytes = sparsemem_required_bytes(memblock);
    early_println!(
        "Reserving {} KiB for sparse metadata...",
        need_bytes / 1024
    );

    let (region_base, _region_size) =
        find_suitable_region(memblock, need_bytes)
            .expect("No suitable usable region for sparsemem");

    let bump_start = align_up(region_base as usize, PAGE_SIZE);
    let bump_end   = bump_start + need_bytes;

    reserve_for_sparsemem(memblock, bump_start as u64, need_bytes as u64)
        .expect("Failed to reserve sparsemem region");

    let bump_virt_start = phys_to_virt(bump_start);
    let bump_virt_end   = phys_to_virt(bump_end);

    let mut bump   = BumpState::init(bump_virt_start, bump_virt_end);
    let mut sparse = SparseMem::empty();

    sparse.init_from_memblock(&mut bump, memblock);

    SPARSE_MEMORY.call_once(|| sparse);

    early_println!(
        "Sparse memory model initialized! max_pfn={}",
        sparse_max_pfn_for_log()
    );
}

fn sparse_max_pfn_for_log() -> Pfn {
    SPARSE_MEMORY
        .get()
        .map(|s| s.max_present_pfn())
        .unwrap_or(0)
}