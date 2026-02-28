#![allow(dead_code)]

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuddyTag {
    Unused    = 0,
    Free      = 1,
    Allocated = 2,
}

use crate::arch::amd64::memory::pmm::sparsemem::{
    Frame, FrameState, INVALID_PFN, Pfn, get_sparse_memory,
};

pub const MAX_ORDER: usize = 11;

#[inline]
fn floor_log2(x: usize) -> usize {
    debug_assert!(x > 0, "floor_log2(0) is undefined");
    usize::BITS as usize - 1 - x.leading_zeros() as usize
}

#[inline]
fn max_block_order_for(start: Pfn, remaining: usize) -> usize {
    if remaining == 0 {
        return 0;
    }
    let max_by_remaining = floor_log2(remaining);
    let tz = if start == 0 {
        MAX_ORDER
    } else {
        core::cmp::min(start.trailing_zeros() as usize, MAX_ORDER)
    };
    core::cmp::min(max_by_remaining, tz)
}

pub struct Buddy {
    base_pfn:   Pfn,
    pages:      usize,
    max_order:  usize,
    free:       [Pfn; MAX_ORDER + 1],
    free_pages: usize,
}

impl Buddy {
    pub fn new(base_pfn: Pfn, pages: usize) -> Self {
        let max_order = if pages == 0 {
            0
        } else {
            core::cmp::min(floor_log2(pages), MAX_ORDER)
        };

        Self {
            base_pfn,
            pages,
            max_order,
            free:       [INVALID_PFN; MAX_ORDER + 1],
            free_pages: 0,
        }
    }

    #[inline] pub fn base_pfn(&self)        -> Pfn    { self.base_pfn }
    #[inline] pub fn page_count(&self)      -> usize  { self.pages }
    #[inline] pub fn free_pages_count(&self)-> usize  { self.free_pages }

    #[inline]
    fn order_pages(order: usize) -> usize { 1usize << order }

    #[inline]
    fn in_range(&self, pfn: Pfn) -> bool {
        pfn >= self.base_pfn && (pfn - self.base_pfn) < self.pages
    }

    #[inline]
    fn local(&self, pfn: Pfn) -> usize { pfn - self.base_pfn }

    #[inline]
    fn to_pfn(&self, local: usize) -> Pfn { self.base_pfn + local }

    #[inline]
    fn buddy_of(&self, head: Pfn, order: usize) -> Option<Pfn> {
        let l     = self.local(head);
        let buddy = l ^ (1usize << order);
        if buddy + Self::order_pages(order) > self.pages {
            return None;
        }
        Some(self.to_pfn(buddy))
    }

    #[inline]
    unsafe fn frame_mut_unchecked(pfn: Pfn) -> &'static mut Frame {
        get_sparse_memory()
            .pfn_to_frame(pfn)
            .expect("buddy: pfn not present in sparsemem")
    }

    #[inline]
    fn is_free_head(&self, pfn: Pfn, order: usize) -> bool {
        if !self.in_range(pfn) {
            return false;
        }
        match get_sparse_memory().pfn_to_frame(pfn) {
            None    => false,
            Some(f) => f.tag == BuddyTag::Free && f.order as usize == order,
        }
    }

    pub fn reset(&mut self) {
        self.free       = [INVALID_PFN; MAX_ORDER + 1];
        self.free_pages = 0;

        for i in 0..self.pages {
            let pfn = self.base_pfn + i;
            if let Some(f) = get_sparse_memory().pfn_to_frame(pfn) {
                f.tag       = BuddyTag::Unused;
                f.order     = 0;
                f.next_free = INVALID_PFN;
                f.prev_free = INVALID_PFN;
            }
        }
    }

    pub fn add_usable_run(&mut self, start: Pfn, len: usize) {
        if len == 0 || self.pages == 0 {
            return;
        }

        let zone_start = self.base_pfn;
        let zone_end   = self.base_pfn + self.pages;

        // Обрезаем по зоне.
        let p_start = core::cmp::max(start, zone_start);
        let p_end   = core::cmp::min(start + len, zone_end);
        if p_start >= p_end {
            return;
        }

        let mut p         = p_start;
        let mut remaining = p_end - p_start;

        while remaining > 0 {
            let order = core::cmp::min(
                max_block_order_for(p, remaining),
                self.max_order,
            );

            self.insert_block_boot(p, order);

            let step = Self::order_pages(order);
            p         += step;
            remaining -= step;
        }
    }

    fn insert_block_boot(&mut self, head: Pfn, order: usize) {
        debug_assert!(order <= self.max_order);
        debug_assert!(self.in_range(head));

        self.mark_block_free_head(head, order);

        let n = Self::order_pages(order);

        self.free_pages += n;

        self.push_free(head, order);
        self.coalesce_up(head, order);
    }

    fn coalesce_up(&mut self, mut head: Pfn, mut order: usize) {
        while order < self.max_order {
            let buddy = match self.buddy_of(head, order) {
                Some(b) => b,
                None    => break,
            };

            if !self.is_free_head(buddy, order) {
                break;
            }

            self.remove_free(head,  order);
            self.remove_free(buddy, order);

            head  = if self.local(head) < self.local(buddy) { head } else { buddy };
            order += 1;

            self.mark_block_free_head(head, order);
            self.push_free(head, order);
        }
    }

    pub fn alloc(&mut self, order: usize) -> Option<Pfn> {
        if order > self.max_order {
            return None;
        }

        let found_order = (order..=self.max_order)
            .find(|&o| self.free[o] != INVALID_PFN)?;

        let head = self.pop_free(found_order)?;

        self.free_pages -= Self::order_pages(found_order);

        let mut cur_order = found_order;
        while cur_order > order {
            cur_order -= 1;

            let buddy = self.buddy_of(head, cur_order)
                .expect("buddy: split produced out-of-range buddy");

            self.mark_block_free_head(buddy, cur_order);
            self.push_free(buddy, cur_order);
            self.free_pages += Self::order_pages(cur_order);
        }

        self.mark_block_allocated(head, order);

        Some(head)
    }

    pub fn free(&mut self, head: Pfn) {
        debug_assert!(self.in_range(head), "buddy.free: pfn out of range");

        let order = {
            let fh = unsafe { Self::frame_mut_unchecked(head) };
            debug_assert!(
                fh.tag == BuddyTag::Allocated,
                "buddy.free: frame is not Allocated"
            );
            fh.tag       = BuddyTag::Free;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
            fh.order as usize
        };

        #[cfg(debug_assertions)]
        for i in 1..Self::order_pages(order) {
            let p  = head + i;
            let f  = unsafe { Self::frame_mut_unchecked(p) };
            debug_assert!(
                f.tag == BuddyTag::Unused,
                "buddy.free: inner frame pfn={} has tag={:?}",
                p, f.tag
            );
        }

        self.free_pages += Self::order_pages(order);
        self.push_free(head, order);
        self.coalesce_up(head, order);
    }

    fn push_free(&mut self, head: Pfn, order: usize) {
        let old = self.free[order];

        unsafe {
            let fh = Self::frame_mut_unchecked(head);
            fh.tag       = BuddyTag::Free;
            fh.order     = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = old;
        }

        if old != INVALID_PFN {
            unsafe { Self::frame_mut_unchecked(old).prev_free = head; }
        }

        self.free[order] = head;
    }

    fn pop_free(&mut self, order: usize) -> Option<Pfn> {
        let head = self.free[order];
        if head == INVALID_PFN {
            return None;
        }

        let next = unsafe {
            let fh       = Self::frame_mut_unchecked(head);
            let next     = fh.next_free;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
            next
        };

        self.free[order] = next;
        if next != INVALID_PFN {
            unsafe { Self::frame_mut_unchecked(next).prev_free = INVALID_PFN; }
        }

        Some(head)
    }

    fn remove_free(&mut self, head: Pfn, order: usize) {
        let (prev, next) = unsafe {
            let fh = Self::frame_mut_unchecked(head);
            debug_assert!(
                fh.tag == BuddyTag::Free && fh.order as usize == order,
                "remove_free: pfn={} tag={:?} order={} expected order={}",
                head, fh.tag, fh.order, order
            );
            let p = fh.prev_free;
            let n = fh.next_free;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
            (p, n)
        };

        if prev != INVALID_PFN {
            unsafe { Self::frame_mut_unchecked(prev).next_free = next; }
        } else {
            self.free[order] = next;
        }

        if next != INVALID_PFN {
            unsafe { Self::frame_mut_unchecked(next).prev_free = prev; }
        }
    }

    fn mark_block_free_head(&mut self, head: Pfn, order: usize) {
        unsafe {
            let fh       = Self::frame_mut_unchecked(head);
            fh.tag       = BuddyTag::Free;
            fh.order     = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        for i in 1..Self::order_pages(order) {
            let p = head + i;
            debug_assert!(
                self.in_range(p),
                "mark_block_free_head: pfn={} out of zone", p
            );
            unsafe {
                let f    = Self::frame_mut_unchecked(p);
                debug_assert!(
                    f.state == FrameState::Usable,
                    "mark_block_free_head: inner pfn={} state={:?}", p, f.state
                );
                f.tag       = BuddyTag::Unused;
                f.order     = 0;
                f.prev_free = INVALID_PFN;
                f.next_free = INVALID_PFN;
            }
        }
    }

    fn mark_block_allocated(&mut self, head: Pfn, order: usize) {
        unsafe {
            let fh       = Self::frame_mut_unchecked(head);
            fh.tag       = BuddyTag::Allocated;
            fh.order     = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }
    }
}