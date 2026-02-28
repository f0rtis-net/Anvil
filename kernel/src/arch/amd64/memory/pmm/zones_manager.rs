#![allow(dead_code)]

use spin::{Mutex, Once};

use crate::{
    arch::amd64::memory::{
        misc::human_readable_size,
        pmm::{
            buddy::Buddy,
            pfn_iterator::UsablePfnRunIter,
            sparsemem::{
                FrameState, PAGE_SHIFT, PAGE_SIZE, PAGES_PER_SECTION,
                Pfn, SECTION_SHIFT, SparseMem, get_sparse_memory,
            },
        },
    },
    early_println,
};


const DMA_LIMIT_BYTES: usize = 16 * 1024 * 1024;
const DMA_LIMIT_PFN:   usize = DMA_LIMIT_BYTES >> PAGE_SHIFT;

pub const MAX_ZONES: usize = 3;

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneId {
    Dma    = 0,
    Normal = 1,
    High   = 2,
}

impl ZoneId {
    #[inline]
    pub const fn idx(self) -> usize {
        self as usize
    }
}

pub struct Zone {
    id:         ZoneId,
    base_pfn:   Pfn,
    page_count: usize,
    allocator:  Buddy,
}

impl Zone {
    fn new(id: ZoneId, base_pfn: Pfn, page_count: usize, allocator: Buddy) -> Self {
        Self { id, base_pfn, page_count, allocator }
    }

    #[inline]
    pub fn id(&self) -> ZoneId { self.id }

    #[inline]
    pub fn base_pfn(&self) -> Pfn { self.base_pfn }

    #[inline]
    pub fn page_count(&self) -> usize { self.page_count }

    #[inline]
    pub fn contains_pfn(&self, pfn: Pfn) -> bool {
        pfn >= self.base_pfn && pfn < self.base_pfn + self.page_count
    }

    pub fn alloc(&mut self, order: usize) -> Option<Pfn> {
        self.allocator.alloc(order)
    }

    pub fn free(&mut self, pfn: Pfn) {
        debug_assert!(self.contains_pfn(pfn), "Zone::free: pfn={} not in zone {:?}", pfn, self.id);
        self.allocator.free(pfn);
    }

    #[inline]
    pub fn free_pages(&self) -> usize {
        self.allocator.free_pages_count()
    }

    pub fn usable_pages(&self) -> usize {
        let sparse   = get_sparse_memory();
        let zone_end = self.base_pfn + self.page_count;

        UsablePfnRunIter::new(sparse)
            .map(|run| {
                let s = core::cmp::max(run.start, self.base_pfn);
                let e = core::cmp::min(run.end(),  zone_end);
                if s < e { e - s } else { 0 }
            })
            .sum()
    }
}

pub struct ZonesManager {
    zones: [Option<Zone>; MAX_ZONES],
}

impl ZonesManager {
    pub const fn new() -> Self {
        Self {
            zones: [const { None }; MAX_ZONES],
        }
    }

    pub fn set_zone(&mut self, zone: Zone) {
        let idx = zone.id().idx();
        assert!(idx < MAX_ZONES, "set_zone: idx {} out of bounds", idx);
        assert!(
            self.zones[idx].is_none(),
            "set_zone: zone {:?} already registered",
            zone.id()
        );
        self.zones[idx] = Some(zone);
    }

    #[inline]
    pub fn zone(&self, id: ZoneId) -> Option<&Zone> {
        self.zones[id.idx()].as_ref()
    }

    #[inline]
    pub fn zone_mut(&mut self, id: ZoneId) -> Option<&mut Zone> {
        self.zones[id.idx()].as_mut()
    }

    pub fn alloc_pages(&mut self, id: ZoneId, order: usize) -> Option<Pfn> {
        self.zone_mut(id)?.alloc(order)
    }

    pub fn free_pages(&mut self, pfn: Pfn) {
        let frame = get_sparse_memory()
            .pfn_to_frame(pfn)
            .expect("free_pages: pfn not present in sparsemem");

        debug_assert!(
            frame.state != FrameState::Absent,
            "free_pages: pfn={} is Absent", pfn
        );
        debug_assert!(
            frame.state != FrameState::Reserved,
            "free_pages: pfn={} is Reserved", pfn
        );

        let zid = frame.zone;
        self.zone_mut(zid)
            .unwrap_or_else(|| panic!("free_pages: zone {:?} not initialized", zid))
            .free(pfn);
    }
}


static ZONES_MANAGER: Once<Mutex<ZonesManager>> = Once::new();

#[inline]
pub fn get_zones_manager() -> &'static Mutex<ZonesManager> {
    ZONES_MANAGER.get().expect("ZonesManager not initialized")
}

fn assign_zone_to_run(sparse: &SparseMem, zid: ZoneId, start: Pfn, len: usize) {
    if len == 0 {
        return;
    }

    let end    = start + len;
    let mut p  = start;

    while p < end {
        let sec      = SparseMem::pfn_to_section(p);
        let sec_base = sec << (SECTION_SHIFT - PAGE_SHIFT);
        let sec_end  = sec_base + PAGES_PER_SECTION;

        let section = unsafe { &*sparse.sections.add(sec) };
        debug_assert!(section.present, "assign_zone_to_run: section {} not present", sec);

        let chunk_end = core::cmp::min(end, sec_end);
        let off       = p - sec_base;
        let n         = chunk_end - p;

        unsafe {
            let frames = section.frames.add(off);
            let slice  = core::slice::from_raw_parts_mut(frames, n);
            for f in slice.iter_mut() {
                if f.state == FrameState::Usable {
                    f.zone = zid;
                }
            }
        }

        p = chunk_end;
    }
}

fn build_zone(id: ZoneId, pfn_start: Pfn, pfn_end: Pfn) -> Zone {
    assert!(pfn_start <= pfn_end, "build_zone: pfn_start > pfn_end");

    let page_count = pfn_end - pfn_start;
    let sparse     = get_sparse_memory();

    let mut allocator = Buddy::new(pfn_start, page_count);
    allocator.reset();

    for run in UsablePfnRunIter::new(sparse) {
        if run.start >= pfn_end {
            break;
        }

        let clipped_start = core::cmp::max(run.start, pfn_start);
        let clipped_end   = core::cmp::min(run.end(),  pfn_end);

        if clipped_start >= clipped_end {
            continue;
        }

        let clipped_len = clipped_end - clipped_start;

        allocator.add_usable_run(clipped_start, clipped_len);
        assign_zone_to_run(sparse, id, clipped_start, clipped_len);
    }

    Zone::new(id, pfn_start, page_count, allocator)
}


fn log_zones_summary() {
    let mgr = get_zones_manager().lock();

    early_println!("\n============ Zones Manager summary ============");

    for &zid in &[ZoneId::Dma, ZoneId::Normal, ZoneId::High] {
        early_println!("\n  Zone: {:?}", zid);

        match mgr.zone(zid) {
            None => {
                early_println!("    (not initialized)");
                continue;
            }
            Some(zone) => {
                let usable     = zone.usable_pages();
                let free       = zone.free_pages();
                let total_size = human_readable_size((usable * PAGE_SIZE) as u64);
                let free_size  = human_readable_size((free  * PAGE_SIZE) as u64);

                early_println!(
                    "    PFN range:    [{:#x} .. {:#x})",
                    zone.base_pfn(),
                    zone.base_pfn() + zone.page_count()
                );
                early_println!(
                    "    Usable pages: {}  ({} {})",
                    usable, total_size.value, total_size.unit.as_str()
                );
                early_println!(
                    "    Free pages:   {}  ({} {})",
                    free, free_size.value, free_size.unit.as_str()
                );
            }
        }
    }

    early_println!("\n===============================================\n");
}

pub fn init_zones_manager() {
    early_println!("Initializing zones manager...");

    let sparse  = get_sparse_memory();
    let max_pfn = sparse.max_present_pfn();

    assert!(
        max_pfn > 0,
        "init_zones_manager: no present memory in sparsemem"
    );
    assert!(
        DMA_LIMIT_PFN <= max_pfn,
        "init_zones_manager: DMA limit {:#x} > max_pfn {:#x}",
        DMA_LIMIT_PFN, max_pfn
    );

    let dma_zone  = build_zone(ZoneId::Dma,  0,             DMA_LIMIT_PFN);
    let high_zone = build_zone(ZoneId::High, DMA_LIMIT_PFN, max_pfn);

    let mut mgr = ZonesManager::new();
    mgr.set_zone(dma_zone);
    mgr.set_zone(high_zone);

    ZONES_MANAGER.call_once(|| Mutex::new(mgr));

    log_zones_summary();

    early_println!("Zones manager initialized.");
}