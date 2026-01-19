use spin::{Mutex, Once};

use crate::{arch::amd64::memory::{misc::human_readable_size, pmm::{buddy::Buddy, pfn_iterator::PfnRunIter, sparsemem::{FrameState, PAGE_SHIFT, PAGE_SIZE, PAGES_PER_SECTION, Pfn, SECTION_SHIFT, SparseMem, get_sparse_memory}}}, serial_println};

const MAX_ZONES: usize = 3;
#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ZoneId {
    Dma = 0,
    Normal = 1,
    High = 2,
}

fn get_zones() -> [ZoneId; MAX_ZONES] {
    return [ZoneId::Dma, ZoneId::High, ZoneId::Normal]
}

static ZONES_MANAGER: Once<Mutex<ZonesManager>> = Once::new();

#[inline]
pub fn get_zones_manager() -> &'static Mutex<ZonesManager> {
    ZONES_MANAGER
        .get()
        .expect("ZonesManager not initialized")
}

#[derive(Clone, Copy)]
pub struct Zone {
    id: ZoneId,
    base_pfn: Pfn,
    page_count: usize,
    allocator: Buddy,
}

impl ZoneId {
    #[inline]
    pub const fn idx(self) -> usize {
        self as usize
    }
}

impl Zone {
    pub fn new(id: ZoneId, base_pfn: Pfn, page_count: usize, allocator: Buddy) -> Self {
        Self { id, base_pfn, page_count, allocator }
    }

    #[inline]
    pub fn id(&self) -> ZoneId {
        self.id
    }

    #[inline]
    pub fn contains_pfn(&self, pfn: Pfn) -> bool {
        pfn >= self.base_pfn && pfn < self.base_pfn + self.page_count
    }

    pub fn alloc(&mut self, order: usize) -> Option<Pfn> {
        self.allocator.alloc(order)
    }

    pub fn free(&mut self, pfn: Pfn) {
        debug_assert!(self.contains_pfn(pfn));
        self.allocator.free(pfn);
    }

    pub fn total_pages(&self) -> usize {
        self.page_count
    }

    pub fn usable_pages(&self) -> usize {
        let mut pages = 0;
        let sparse = get_sparse_memory();
        for run in PfnRunIter::new(sparse) {
            let rs = run.start;
            let re = run.start + run.len;

            let zs = self.base_pfn;
            let ze = self.base_pfn + self.page_count;

            let s = core::cmp::max(rs, zs);
            let e = core::cmp::min(re, ze);

            if s < e {
                pages += e - s;
            }
        }

        pages
    }

    pub fn free_pages(&self) -> usize {
        self.allocator.free_pages_count()
    }
}

pub struct ZonesManager {
    zones: [Option<Zone>; MAX_ZONES],
}

impl ZonesManager {
    pub const fn new() -> Self {
        Self {
            zones: [None; MAX_ZONES],
        }
    }

    pub fn set_zone(&mut self, zone: Zone) {
        let idx = zone.id().idx();
        assert!(idx < MAX_ZONES);
        assert!(self.zones[idx].is_none(), "zone {:?} already set", zone.id());
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
        if let Some(z) = self.zone_mut(id) {
            if let Some(pfn) = z.alloc(order) {
                return Some(pfn);
            }
        }
        None
    }

    pub fn free_pages(&mut self, pfn: Pfn) {
        let f = get_sparse_memory().pfn_to_frame(pfn).expect("free_pages: pfn not present");
        debug_assert!(f.state != FrameState::Absent, "free_pages: absent page");
        debug_assert!(f.state != FrameState::Reserved, "free_pages: reserved page");

        let zid = f.zone;
        let z = self.zone_mut(zid).unwrap_or_else(|| panic!("free_pages: zone {:?} not initialized", zid));
        z.free(pfn);
    }
}

fn set_zone_for_run(sparse: &SparseMem, zid: ZoneId, start: Pfn, len: usize) {
    let end = start + len;

    let mut p = start;
    while p < end {
        let sec = SparseMem::pfn_to_section(p);            
        let sec_base = sec << (SECTION_SHIFT - PAGE_SHIFT);
        let sec_end  = sec_base + PAGES_PER_SECTION;

        let chunk_end = core::cmp::min(end, sec_end);
        let off = p - sec_base;
        let n = chunk_end - p;

        let section = &sparse.sections()[sec];
        debug_assert!(section.present);

        unsafe {
            let frames = section.frames.add(off);
            let slice = core::slice::from_raw_parts_mut(frames, n);

            for f in slice.iter_mut() {
                if f.state == FrameState::Usable {
                    f.zone = zid;
                }
            }
        }

        p = chunk_end;
    }
}

fn make_zone(id: ZoneId, pfn_start: Pfn, pfn_end: Pfn) -> Zone {
    let page_count = pfn_end.saturating_sub(pfn_start);

    let sparse = get_sparse_memory(); 

    let mut allocator = Buddy::new(pfn_start, page_count);
    allocator.reset();

    for run in PfnRunIter::new(sparse) {
        if run.start >= pfn_end {
            break;
        }

        let rs = run.start;
        let re = run.start + run.len;

        let start = core::cmp::max(rs, pfn_start);
        let end   = core::cmp::min(re, pfn_end);

        if start < end {
            let len = end - start;

            allocator.add_usable_run_fast(start, len);
            
            set_zone_for_run(sparse, id, start, len);
        }
    }

    Zone::new(id, pfn_start, page_count, allocator)
}

fn zone_manager_statistics() {
    let zones_manager = get_zones_manager().lock();

    serial_println!("\n============ Zones Manager summary ============");

    for zone_id in get_zones() {
        let zone = zones_manager.zone(zone_id);

        serial_println!("\n Zone: {:?}", zone_id);

        if zone.is_none() {
            serial_println!("   Uninitialized zone!\n");
            continue;
        }

        let zone = zone.unwrap();

        let zone_page_count = zone.usable_pages();
        let zone_size = human_readable_size((zone_page_count * PAGE_SIZE) as u64);

        serial_println!("   Pages count:             {}", zone_page_count);
        serial_println!("   Zone total size:         {} {}", zone_size.value, zone_size.unit.as_str());
    }

    serial_println!("============ Zones Manager summary ============\n");
}

pub fn init_zones_manager() {
    serial_println!("Initializing zones manager...");

    let dma_limit_pfn = (16 * 1024 * 1024) >> 12;
    let max_pfn = get_sparse_memory().max_present_pfn();

    assert!(
        dma_limit_pfn <= max_pfn,
        "Cannot init zones: dma_limit_pfn ({:#x}) > max_present_pfn ({:#x})",
        dma_limit_pfn,
        max_pfn
    );

    //TODO: Make guarinted size zone reservation!!!!
    let dma_zone = make_zone(ZoneId::Dma, 0, dma_limit_pfn);
    let high_zone = make_zone(ZoneId::High, dma_limit_pfn, max_pfn);

    let mut mgr = ZonesManager::new();
    mgr.set_zone(dma_zone);
    mgr.set_zone(high_zone);

    ZONES_MANAGER.call_once(|| Mutex::new(mgr));

    zone_manager_statistics();

    serial_println!(" Zones manager initialized");
}