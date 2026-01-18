use crate::arch::amd64::memory::{
    misc::floor_log2,
    pmm::frame_area::{AreaId, FrameFlags, FramesDB, FramesDBComposite, INVALID_PFN, Pfn, frames_db},
};

const MAX_ORDER: usize = 11;

pub struct Buddy {
    max_order: usize,
    free: [Pfn; MAX_ORDER + 1],
    area_id: AreaId,
}

impl Buddy {
    pub fn new(area_id: AreaId) -> Self {
        Self {
            max_order: 0,
            free: [INVALID_PFN; MAX_ORDER + 1],
            area_id,
        }
    }

    #[inline]
    pub fn area_id(&self) -> AreaId {
        self.area_id
    }

    // ---------------- locked helpers ----------------

    #[inline]
    fn base(&self, db: &FramesDBComposite) -> Pfn {
        db.area(self.area_id).base_pfn()
    }

    #[inline]
    fn count(&self, db: &FramesDBComposite) -> usize {
        db.area(self.area_id).page_count()
    }

    #[inline]
    fn in_range(&self, db: &FramesDBComposite, pfn: Pfn) -> bool {
        db.area(self.area_id).contains(pfn)
    }

    #[inline]
    fn pfn_to_local(&self, db: &FramesDBComposite, pfn: Pfn) -> usize {
        pfn - self.base(db)
    }

    #[inline]
    fn local_to_pfn(&self, db: &FramesDBComposite, local: usize) -> Pfn {
        self.base(db) + local
    }

    #[inline]
    fn buddy_of(&self, db: &FramesDBComposite, head: Pfn, order: usize) -> Pfn {
        let local = self.pfn_to_local(db, head);
        let buddy_local = local ^ (1usize << order);
        self.local_to_pfn(db, buddy_local)
    }

    #[inline]
    fn head_min_by_local(&self, db: &FramesDBComposite, a: Pfn, b: Pfn) -> Pfn {
        if self.pfn_to_local(db, a) <= self.pfn_to_local(db, b) { a } else { b }
    }

    // ---------------- public API ----------------

    pub fn init(&mut self) {
        let mut guard = frames_db();
        let db: &mut FramesDBComposite = &mut *guard;

        let pages = self.count(db);
        self.free = [INVALID_PFN; MAX_ORDER + 1];

        if pages == 0 {
            self.max_order = 0;
            return;
        }

        self.max_order = core::cmp::min(floor_log2(pages), MAX_ORDER);

        let mut local = 0usize;
        while local < pages {
            let remain = pages - local;
            let mut order = floor_log2(remain).min(self.max_order);

            while order > 0 && (local & ((1usize << order) - 1)) != 0 {
                order -= 1;
            }

            let head = self.local_to_pfn(db, local);
            self.push_free(db, head, order);

            local += 1usize << order;
        }
    }

    pub fn alloc_pfns(&mut self, order: usize) -> Option<Pfn> {
        let mut guard = frames_db();
        let db: &mut FramesDBComposite = &mut *guard;
        self.alloc_order(db, order)
    }

    pub fn free_pfn(&mut self, pfn: Pfn) {
        let mut guard = frames_db();
        let db: &mut FramesDBComposite = &mut *guard;

        debug_assert!(self.in_range(db, pfn));

        let f = db.area(self.area_id).frame(pfn);

        debug_assert!(
            f.flags == FrameFlags::Allocated,
            "free_pfn: pfn {:#x} is not allocated",
            pfn
        );

        debug_assert!(
            (f.order as usize) <= self.max_order,
            "free_pfn: invalid order {} for pfn {:#x}",
            f.order,
            pfn
        );

        self.free_block(db, pfn);
    }

    fn alloc_order(&mut self, db: &mut FramesDBComposite, order: usize) -> Option<Pfn> {
        if order > self.max_order {
            return None;
        }

        let mut cur = order;
        while cur <= self.max_order && self.free[cur] == INVALID_PFN {
            cur += 1;
        }
        if cur > self.max_order {
            return None;
        }

        let head = self.pop_free(db, cur)?;

        let mut cur_order = cur;
        while cur_order > order {
            cur_order -= 1;
            let buddy = self.buddy_of(db, head, cur_order);

            debug_assert!(self.in_range(db, buddy));
            self.push_free(db, buddy, cur_order);
        }

        // mark allocated head
        {
            let f = db.area_mut(self.area_id).frame_mut(head);
            f.flags = FrameFlags::Allocated;
            f.order = order as u8;
            f.owner = 0;
            f.prev_free = INVALID_PFN;
            f.next_free = INVALID_PFN;
        }

        // mark interior pages as Unused
        let n = 1usize << order;
        for i in 1..n {
            let p = head + i;
            if !self.in_range(db, p) {
                break;
            }

            let fi = db.area_mut(self.area_id).frame_mut(p);
            fi.flags = FrameFlags::Unused;
            fi.order = 0;
            fi.owner = 0;
            fi.prev_free = INVALID_PFN;
            fi.next_free = INVALID_PFN;
        }

        Some(head)
    }

    fn free_block(&mut self, db: &mut FramesDBComposite, mut head: Pfn) {
        let f0 = db.area(self.area_id).frame(head);
        let mut order = f0.order as usize;

        {
            let f = db.area_mut(self.area_id).frame_mut(head);
            f.flags = FrameFlags::Free;
            f.order = order as u8;
            f.prev_free = INVALID_PFN;
            f.next_free = INVALID_PFN;
        }

        while order < self.max_order {
            let buddy = self.buddy_of(db, head, order);

            if !self.in_range(db, buddy) {
                break;
            }
            if !self.is_free_head(db, buddy, order) {
                break;
            }
            if !self.remove_free(db, buddy, order) {
                break;
            }

            head = self.head_min_by_local(db, head, buddy);
            order += 1;

            let f = db.area_mut(self.area_id).frame_mut(head);
            f.flags = FrameFlags::Free;
            f.order = order as u8;
            f.prev_free = INVALID_PFN;
            f.next_free = INVALID_PFN;
        }

        self.push_free(db, head, order);
    }

    fn is_free_head(&self, db: &FramesDBComposite, head: Pfn, order: usize) -> bool {
        let f = db.area(self.area_id).frame(head);
        f.flags == FrameFlags::Free && (f.order as usize == order)
    }

    fn push_free(&mut self, db: &mut FramesDBComposite, head: Pfn, order: usize) {
        debug_assert!(order <= self.max_order);
        debug_assert!(self.in_range(db, head));

        let old = self.free[order];

        {
            let fh = db.area(self.area_id).frame(head);
            debug_assert!(
                fh.prev_free == INVALID_PFN && fh.next_free == INVALID_PFN,
                "push_free: head {:#x} already linked (prev={:#x}, next={:#x})",
                head, fh.prev_free, fh.next_free
            );
        }

        {
            let f = db.area_mut(self.area_id).frame_mut(head);
            f.flags = FrameFlags::Free;
            f.order = order as u8;
            f.owner = 0;
            f.prev_free = INVALID_PFN;
            f.next_free = old;
        }

        if old != INVALID_PFN {
            db.area_mut(self.area_id).frame_mut(old).prev_free = head;
        }

        self.free[order] = head;
    }

    fn pop_free(&mut self, db: &mut FramesDBComposite, order: usize) -> Option<Pfn> {
        let head = self.free[order];
        if head == INVALID_PFN {
            return None;
        }

        let next = db.area(self.area_id).frame(head).next_free;
        self.free[order] = next;

        if next != INVALID_PFN {
            db.area_mut(self.area_id).frame_mut(next).prev_free = INVALID_PFN;
        }

        let f = db.area_mut(self.area_id).frame_mut(head);
        f.prev_free = INVALID_PFN;
        f.next_free = INVALID_PFN;

        Some(head)
    }

    fn remove_free(&mut self, db: &mut FramesDBComposite, head: Pfn, order: usize) -> bool {
        let f = db.area(self.area_id).frame(head);

        debug_assert!(
            f.flags == FrameFlags::Free && (f.order as usize == order),
            "remove_free: invalid block head={:#x}",
            head
        );

        let prev = f.prev_free;
        let next = f.next_free;

        if prev != INVALID_PFN {
            db.area_mut(self.area_id).frame_mut(prev).next_free = next;
        } else {
            self.free[order] = next;
        }

        if next != INVALID_PFN {
            db.area_mut(self.area_id).frame_mut(next).prev_free = prev;
        }

        let f = db.area_mut(self.area_id).frame_mut(head);
        f.prev_free = INVALID_PFN;
        f.next_free = INVALID_PFN;

        true
    }
}

#[inline]
pub fn pages_to_order(pages: usize) -> usize {
    if pages <= 1 {
        0
    } else {
        (usize::BITS - (pages - 1).leading_zeros()) as usize
    }
}
