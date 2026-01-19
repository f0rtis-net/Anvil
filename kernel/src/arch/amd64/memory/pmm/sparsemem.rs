#![allow(dead_code)]

use core::ptr::null_mut;

use spin::Once;

use crate::{arch::amd64::memory::{misc::{align_up, phys_to_virt}, pmm::{HHDM_OFFSET, buddy::BuddyTag, bump_alloc::BumpState, memblock::{Memblock, MemblockError, MemblockType}, zones_manager::ZoneId}}, serial_println};

pub const PAGE_SHIFT: usize = 12;
pub const PAGE_SIZE: usize = 1 << PAGE_SHIFT;

pub const SECTION_SHIFT: usize = 27; // 128 MiB
pub const SECTION_SIZE: usize = 1 << SECTION_SHIFT;

pub const PAGES_PER_SECTION: usize = 1 << (SECTION_SHIFT - PAGE_SHIFT);

pub type Pfn = usize;
pub const INVALID_PFN: Pfn = Pfn::MAX;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FrameState {
    Absent   = 0, // no RAM / hole
    Usable   = 1, // usable RAM
    Reserved = 2, // kernel / firmware / ACPI / etc
}

#[repr(C)]
#[derive(Clone, Copy)]
pub struct Frame {
    pub state: FrameState,
    pub order: u8,
    pub tag: BuddyTag,
    pub zone: ZoneId,
    pub next_free: Pfn,  
    pub prev_free: Pfn,  
}

impl Frame {
    #[inline]
    pub const fn absent() -> Self {
        Self { 
            state: FrameState::Absent,
            order: 0,
            tag: BuddyTag::Unused,
            zone: ZoneId::Normal,
            next_free: INVALID_PFN,
            prev_free: INVALID_PFN
        }
    }
}

#[repr(C)]
pub struct Memsection {
    pub frames: *mut Frame,
    pub present: bool,
}

impl Memsection {
    #[inline]
    pub const fn empty() -> Self {
        Self {
            frames: null_mut(),
            present: false,
        }
    }
}

static SPARSE_MEMORY: Once<SparseMem> = Once::new();

#[inline]
pub fn get_sparse_memory() -> &'static SparseMem {
    SPARSE_MEMORY.get()
        .expect("Sparsemem not initialized")
}

pub struct SparseMem {
    pub sections: *mut Memsection,
    pub section_count: usize,
    pub max_present_sec: usize,
}

unsafe impl Send for SparseMem {}
unsafe impl Sync for SparseMem {}

impl SparseMem {
    pub const fn empty() -> Self {
        Self {
            sections: null_mut(),
            section_count: 0,
            max_present_sec: 0
        }
    }

    #[inline]
    pub fn is_initialized(&self) -> bool {
        !self.sections.is_null()
    }

    #[inline]
    fn sections_mut(&mut self) -> &mut [Memsection] {
        unsafe {
            core::slice::from_raw_parts_mut(self.sections, self.section_count)
        }
    }

    #[inline]
    pub fn sections(&self) -> &[Memsection] {
        unsafe {
            core::slice::from_raw_parts(self.sections, self.section_count)
        }
    }

    #[inline]
    pub fn pfn_to_section(pfn: Pfn) -> usize {
        (pfn << PAGE_SHIFT) >> SECTION_SHIFT
    }

    #[inline]
    fn pfn_to_offset(pfn: Pfn) -> usize {
        pfn & (PAGES_PER_SECTION - 1)
    }

    #[inline]
    pub fn pfn_present(&self, pfn: Pfn) -> bool {
        let sec = Self::pfn_to_section(pfn);
        sec < self.section_count && self.sections()[sec].present
    }

    pub fn max_present_pfn(&self) -> Pfn {
        let mut max = 0;

        for sec in 0..self.section_count {
            let s = &self.sections()[sec];
            if !s.present {
                continue;
            }

            let base = sec << (SECTION_SHIFT - PAGE_SHIFT);

            for i in 0..PAGES_PER_SECTION {
                let pfn = base + i;
                let f = unsafe { &*s.frames.add(i) };

                if f.state != FrameState::Absent {
                    max = core::cmp::max(max, pfn + 1);
                }
            }
        }

        max
    }


    #[inline]
    pub fn pfn_to_frame(&self, pfn: Pfn) -> Option<&'static mut Frame> {
        if !self.pfn_present(pfn) {
            return None;
        }

        let sec = Self::pfn_to_section(pfn);
        let off = Self::pfn_to_offset(pfn);

        unsafe {
            Some(&mut *self.sections()[sec].frames.add(off))
        }
    }

    pub fn init_from_memblock(
        &mut self,
        bump: &mut BumpState,
        memblock: &Memblock,
    ) {
        debug_assert!(!self.is_initialized());

        let max_phys = memblock.max_phys_addr() as usize;
        if max_phys == 0 {
            return;
        }

        let section_count = ((max_phys - 1) >> SECTION_SHIFT) + 1;

        let bytes = section_count * core::mem::size_of::<Memsection>();
        let align = core::mem::align_of::<Memsection>();

        let ptr = bump
            .alloc_zeroed(bytes, align)
            .expect("OOM: sparsemem section table")
            .as_ptr() as *mut Memsection;

        for i in 0..section_count {
            unsafe { ptr.add(i).write(Memsection::empty()); }
        }

        self.sections = ptr;
        self.section_count = section_count;

        for r in memblock.memory_regions() {
            if r.kind != MemblockType::Usable || r.size == 0 {
                continue;
            }
            self.mark_range(
                bump,
                r.base as usize,
                r.end() as usize,
                FrameState::Usable,
                true,
            );
        }

        for r in memblock.reserved_regions() {
            if r.size == 0 {
                continue;
            }
            self.mark_range(
                bump,
                r.base as usize,
                r.end() as usize,
                FrameState::Reserved,
                false,
            );
        }
    }

    fn ensure_section(&mut self, bump: &mut BumpState, sec: usize) {
        if sec >= self.section_count {
            return;
        }

        if self.sections()[sec].present {
            return;
        }

        let bytes = PAGES_PER_SECTION * core::mem::size_of::<Frame>();
        let align = core::mem::align_of::<Frame>();

        let frames = bump
            .alloc_zeroed(bytes, align)
            .expect("OOM: sparsemem frames")
            .as_ptr() as *mut Frame;

        for i in 0..PAGES_PER_SECTION {
            unsafe {
                frames.add(i).write(Frame::absent());
            }
        }

        let sections = self.sections_mut();
        sections[sec].frames = frames;
        sections[sec].present = true;

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

            if !self.sections()[sec].present {
                let next_sec_pfn =
                    (sec + 1) << (SECTION_SHIFT - PAGE_SHIFT);
                pfn = core::cmp::min(end_pfn, next_sec_pfn);
                continue;
            }

            let sec_base_pfn = sec << (SECTION_SHIFT - PAGE_SHIFT);
            let sec_end_pfn = sec_base_pfn + PAGES_PER_SECTION;

            let chunk_end = core::cmp::min(end_pfn, sec_end_pfn);
            let off = pfn - sec_base_pfn;
            let len = chunk_end - pfn;

            unsafe {
                let frames = self.sections()[sec].frames.add(off);
                let slice = core::slice::from_raw_parts_mut(frames, len);
                for f in slice {
                    f.state = state;
                }
            }

            pfn = chunk_end;
        }
    }
}

fn section_count_from_max_phys(max_phys: usize) -> usize {
    if max_phys == 0 {
        0
    } else {
        ((max_phys - 1) >> SECTION_SHIFT) + 1
    }
}

fn count_present_sections(memblock: &Memblock) -> usize {
    let mut count = 0;

    for (i, r) in memblock.memory_regions().iter().enumerate() {
        if r.size == 0 {
            continue;
        }

        let start_sec = (r.base as usize) >> SECTION_SHIFT;
        let end_sec   = ((r.end() as usize) - 1) >> SECTION_SHIFT;

        for sec in start_sec..=end_sec {
            let mut seen_before = false;

            for prev in memblock.memory_regions().iter().take(i) {
                if prev.size == 0 {
                    continue;
                }

                let ps = (prev.base as usize) >> SECTION_SHIFT;
                let pe = ((prev.end() as usize) - 1) >> SECTION_SHIFT;

                if sec >= ps && sec <= pe {
                    seen_before = true;
                    break;
                }
            }

            if !seen_before {
                count += 1;
            }
        }
    }

    count
}


fn sparsemem_required_bytes(memblock: &Memblock) -> usize {
    let max_phys = memblock.max_phys_addr() as usize;

    let section_count = section_count_from_max_phys(max_phys);
    let present_sections = count_present_sections(memblock);

    let bytes_sections =
        section_count * core::mem::size_of::<Memsection>();

    let bytes_frames =
        present_sections
            * PAGES_PER_SECTION
            * core::mem::size_of::<Frame>();

    bytes_sections + bytes_frames
}

fn find_largest_usable_region(memblock: &Memblock) -> Option<(u64, u64)> {
    let mut best: Option<(u64, u64)> = None;

    for r in memblock.memory_regions() {
        let size = r.size;
        match best {
            None => best = Some((r.base, size)),
            Some((_, best_size)) if size > best_size => {
                best = Some((r.base, size))
            }
            _ => {}
        }
    }

    best
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
    serial_println!("Initializing sparse memory model...");
    let need_bytes = sparsemem_required_bytes(memblock);

    serial_println!("Reserving {} MiB for sparse metadata...", need_bytes / 1024 / 1024);

    let (region_base, region_size) =
        find_largest_usable_region(memblock)
            .expect("No usable memory for sparsemem");

    let bump_start = align_up(region_base as usize, PAGE_SIZE);
    let bump_end   = bump_start as u64 + need_bytes as u64;

    assert!(
        bump_end <= region_base + region_size,
        "Largest region too small for sparsemem"
    );

    reserve_for_sparsemem(memblock, bump_start as u64, need_bytes as u64)
        .expect("Failed to reserve sparsemem region");

    let bump_virt_start = phys_to_virt(unsafe { HHDM_OFFSET }, bump_start);
    let bump_virt_end = phys_to_virt(unsafe { HHDM_OFFSET }, bump_end as usize);

    let mut bump = BumpState::init(
        bump_virt_start,
        bump_virt_end,
    );

    let mut sparse = SparseMem::empty();

    sparse.init_from_memblock(&mut bump, memblock);

    SPARSE_MEMORY.call_once(|| {
        sparse
    });

    serial_println!("Sparse memory model initialized!");
}
