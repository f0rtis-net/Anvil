use crate::arch::amd64::memory::pmm::{
    frame_alloc::Buddy,
    frame_area::{AreaId, FramesDB, Pfn, frames_db},
};

const MAX_ZONE_BUDDIES: usize = 64;

#[derive(Clone, Copy)]
pub enum MemoryZoneType {
    High,
    Dma,
}

pub struct MemoryZone {
    kind: MemoryZoneType,
    buddies: [Option<Buddy>; MAX_ZONE_BUDDIES],
    buddy_cnt: usize,
}

impl MemoryZone {
    pub fn new(kind: MemoryZoneType) -> Self {
        Self {
            kind,
            buddies: [(); MAX_ZONE_BUDDIES].map(|_| None),
            buddy_cnt: 0,
        }
    }

    #[inline]
    pub fn kind(&self) -> MemoryZoneType {
        self.kind
    }

    pub fn add_area(&mut self, area_id: AreaId) {
        assert!(self.buddy_cnt < MAX_ZONE_BUDDIES, "Too many buddies in zone");

        let buddy = Buddy::new(area_id);
        self.buddies[self.buddy_cnt] = Some(buddy);
        self.buddy_cnt += 1;
    }

    pub fn init(&mut self) {
        for i in 0..self.buddy_cnt {
            if let Some(b) = &mut self.buddies[i] {
                b.init();
            }
        }
    }

    pub fn alloc_order(&mut self, order: usize) -> Option<Pfn> {
        for i in 0..self.buddy_cnt {
            if let Some(buddy) = &mut self.buddies[i] {
                if let Some(pfn) = buddy.alloc_pfns(order) {
                    let mut db = frames_db();
                    let f = db.area_mut(buddy.area_id()).frame_mut(pfn);
                    f.owner = i;
                    return Some(pfn);
                }
            }
        }
        None
    }
 
    pub fn free(&mut self, pfn: Pfn) {
        let owner = {
            let db = frames_db();
            let area_id = db
                .find_area_id(pfn)
                .expect("MemoryZone::free: PFN not in any FramesArea");

            let f = db.area(area_id).frame(pfn);
            f.owner
        };

        let owner = owner as usize;
        assert!(owner < self.buddy_cnt, "MemoryZone::free: invalid owner");

        let buddy = self.buddies[owner]
            .as_mut()
            .expect("MemoryZone::free: buddy slot empty");

        buddy.free_pfn(pfn);
    }
}
