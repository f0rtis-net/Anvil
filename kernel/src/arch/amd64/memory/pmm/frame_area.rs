use core::{mem::MaybeUninit, ptr::null_mut};

use spin::{Mutex, MutexGuard, Once};

use crate::arch::amd64::memory::pmm::mem_zones::MemoryZoneType;

pub const FRAME_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;
pub const MAX_FRAMES_AREAS: usize = 64;
pub const INVALID_PFN: Pfn = Pfn::MAX;

pub type Pfn = usize;
pub type AreaId = usize;

static FRAMES_DB: Once<Mutex<FramesDBComposite>> = Once::new();

#[inline]
pub fn frames_db() -> MutexGuard<'static, FramesDBComposite> {
    FRAMES_DB
        .get()
        .expect("FRAMES_DB not initialized")
        .lock()
}

#[inline]
pub fn pfn_to_physical(pfn: usize) -> usize {
    pfn << PAGE_SHIFT
}

#[inline]
pub fn physical_to_pfn(ptr: usize) -> usize {
    ptr >> PAGE_SHIFT
}

#[derive(PartialEq, Eq, Copy, Clone)]
pub enum FrameFlags {
    Allocated = 1,
    Free,
    Unused,
}

#[repr(C)]
pub struct Frame {
    pub flags: FrameFlags,
    pub order: u8, 
    pub owner: usize,
    pub zone: MemoryZoneType,
    pub next_free: Pfn,
    pub prev_free: Pfn,
}

pub trait FramesDB {
    fn base_pfn(&self) -> Pfn;
    fn page_count(&self) -> usize;

    fn frame(&self, pfn: Pfn) -> &Frame;
    fn frame_mut(&mut self, pfn: Pfn) -> &mut Frame;
}

#[derive(Clone, Copy)]
pub struct FramesArea {
    frames_metadata: *mut Frame,
    base_pfn: Pfn,
    pages_count: usize,
}

unsafe impl Send for FramesArea {}
unsafe impl Sync for FramesArea {}

impl FramesArea {
    pub fn empty() -> Self {
        Self {
            frames_metadata: null_mut(),
            base_pfn: 0,
            pages_count: 0,
        }
    }

    #[inline]
    pub fn base_pfn(&self) -> Pfn {
        self.base_pfn
    }

    #[inline]
    pub fn page_count(&self) -> usize {
        self.pages_count
    }

    #[inline]
    pub fn contains(&self, pfn: Pfn) -> bool {
        pfn >= self.base_pfn && (pfn - self.base_pfn) < self.pages_count
    }

    pub fn init(
        &mut self,
        buf: *mut Frame,
        zone: MemoryZoneType,
        base_pfn: Pfn,
        count: usize,
    ) {
        assert!(!buf.is_null(), "FramesArea::init: buf is null");
        assert!(count > 0, "FramesArea::init: count is 0");

        self.frames_metadata = buf;
        self.base_pfn = base_pfn;
        self.pages_count = count;

        for i in 0..count {
            unsafe {
                *buf.add(i) = Frame {
                    flags: FrameFlags::Free,
                    order: 0,
                    owner: 0,
                    zone,
                    next_free: INVALID_PFN,
                    prev_free: INVALID_PFN,
                };
            }
        }
    }

    #[inline]
    fn idx(&self, pfn: Pfn) -> usize {
        debug_assert!(self.contains(pfn));
        pfn - self.base_pfn
    }

    #[inline]
    fn ptr(&self, pfn: Pfn) -> *mut Frame {
        debug_assert!(!self.frames_metadata.is_null(), "FramesArea: metadata null");
        unsafe { self.frames_metadata.add(self.idx(pfn)) }
    }
}

impl FramesDB for FramesArea {
    fn base_pfn(&self) -> Pfn {
        self.base_pfn
    }

    fn page_count(&self) -> usize {
        self.pages_count
    }

    fn frame(&self, pfn: Pfn) -> &Frame {
        debug_assert!(self.contains(pfn), "FramesArea::frame: PFN out of range");
        unsafe { &*self.ptr(pfn) }
    }

    fn frame_mut(&mut self, pfn: Pfn) -> &mut Frame {
        debug_assert!(self.contains(pfn), "FramesArea::frame_mut: PFN out of range");
        unsafe { &mut *self.ptr(pfn) }
    }
}

pub struct FramesDBComposite {
    areas: [FramesArea; MAX_FRAMES_AREAS],
    area_count: usize,
}

unsafe impl Send for FramesDBComposite {}
unsafe impl Sync for FramesDBComposite {}

impl FramesDBComposite {
    pub fn new() -> Self {
        let mut tmp: [MaybeUninit<FramesArea>; MAX_FRAMES_AREAS] =
            unsafe { MaybeUninit::uninit().assume_init() };

        for slot in &mut tmp {
            slot.write(FramesArea::empty());
        }

        let areas = unsafe { core::mem::transmute::<_, [FramesArea; MAX_FRAMES_AREAS]>(tmp) };

        Self {
            areas,
            area_count: 0,
        }
    }


    pub fn add_area(&mut self, area: FramesArea) -> AreaId {
        assert!(
            self.area_count < MAX_FRAMES_AREAS,
            "FramesDBComposite: too many areas"
        );
        debug_assert!(
            !area.frames_metadata.is_null() && area.pages_count > 0,
            "FramesDBComposite::add_area: invalid area"
        );

        let id = self.area_count;
        self.areas[id] = area;
        self.area_count += 1;
        id
    }

    #[inline]
    pub fn area(&self, id: AreaId) -> &FramesArea {
        assert!(id < self.area_count, "FramesDBComposite::area: invalid id");
        &self.areas[id]
    }

    #[inline]
    pub fn area_mut(&mut self, id: AreaId) -> &mut FramesArea {
        assert!(
            id < self.area_count,
            "FramesDBComposite::area_mut: invalid id"
        );
        &mut self.areas[id]
    }

    pub fn find_area_id(&self, pfn: Pfn) -> Option<AreaId> {
        for id in 0..self.area_count {
            if self.areas[id].contains(pfn) {
                return Some(id);
            }
        }
        None
    }

    fn find_area(&self, pfn: Pfn) -> Option<&FramesArea> {
        self.find_area_id(pfn).map(|id| &self.areas[id])
    }

    fn find_area_mut(&mut self, pfn: Pfn) -> Option<&mut FramesArea> {
        let id = self.find_area_id(pfn)?;
        Some(&mut self.areas[id])
    }
}

impl FramesDB for FramesDBComposite {
    fn base_pfn(&self) -> Pfn {
        self.areas[..self.area_count]
            .iter()
            .map(|a| a.base_pfn())
            .min()
            .unwrap_or(0)
    }

    fn page_count(&self) -> usize {
        self.areas[..self.area_count]
            .iter()
            .map(|a| a.page_count())
            .sum()
    }

    fn frame(&self, pfn: Pfn) -> &Frame {
        self.find_area(pfn)
            .expect("FramesDBComposite::frame: PFN not in any FramesArea")
            .frame(pfn)
    }

    fn frame_mut(&mut self, pfn: Pfn) -> &mut Frame {
        self.find_area_mut(pfn)
            .expect("FramesDBComposite::frame_mut: PFN not in any FramesArea")
            .frame_mut(pfn)
    }
}

pub fn initialize_page_database() {
    FRAMES_DB.call_once(|| Mutex::new(FramesDBComposite::new()));
}
