#![allow(dead_code)]

#[repr(u8)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BuddyTag {
    Unused = 0,
    Free = 1,
    Allocated = 2,
}

use crate::arch::amd64::memory::pmm::sparsemem::{
    Frame, FrameState, INVALID_PFN, Pfn, get_sparse_memory,
};

const MAX_ORDER: usize = 11;

#[inline]
fn floor_log2(mut x: usize) -> usize {
    let mut r = 0usize;
    while x > 1 {
        x >>= 1;
        r += 1;
    }
    r
}

#[inline]
fn max_block_order_for(start: Pfn, remaining: usize) -> usize {
    if remaining == 0 {
        return 0;
    }
    let max_by_remaining = floor_log2(remaining);
    let tz = start.trailing_zeros() as usize;
    core::cmp::min(max_by_remaining, tz)
}

#[derive(Clone, Copy)]
pub struct Buddy {
    base_pfn: Pfn,
    pages: usize,

    max_order: usize,
    free: [Pfn; MAX_ORDER + 1],
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
            free: [INVALID_PFN; MAX_ORDER + 1],
            free_pages: 0, // важно: реально свободные появятся после add_usable_run_fast()
        }
    }

    #[inline]
    pub fn base_pfn(&self) -> Pfn { self.base_pfn }

    #[inline]
    pub fn page_count(&self) -> usize { self.pages }

    #[inline]
    pub fn free_pages_count(&self) -> usize { self.free_pages }

    #[inline]
    fn order_to_pages(&self, order: usize) -> usize { 1usize << order }

    #[inline]
    fn in_range(&self, pfn: Pfn) -> bool {
        pfn >= self.base_pfn && (pfn - self.base_pfn) < self.pages
    }

    #[inline]
    fn local(&self, pfn: Pfn) -> usize { pfn - self.base_pfn }

    #[inline]
    fn to_pfn(&self, local: usize) -> Pfn { self.base_pfn + local }

    #[inline]
    fn buddy_of(&self, head: Pfn, order: usize) -> Pfn {
        let l = self.local(head);
        self.to_pfn(l ^ (1usize << order))
    }

    #[inline]
    fn frame_mut(&self, pfn: Pfn) -> &mut Frame {
        get_sparse_memory()
            .pfn_to_frame(pfn)
            .expect("buddy: pfn not present")
    }

    #[inline]
    fn is_usable(&self, pfn: Pfn) -> bool {
        if !self.in_range(pfn) {
            return false;
        }
        match get_sparse_memory().pfn_to_frame(pfn) {
            Some(f) => f.state == FrameState::Usable,
            None => false,
        }
    }

    #[inline]
    fn is_free_head(&self, head: Pfn, order: usize) -> bool {
        if !self.in_range(head) {
            return false;
        }
        match get_sparse_memory().pfn_to_frame(head) {
            None => false,
            Some(f) => f.tag == BuddyTag::Free && (f.order as usize) == order,
        }
    }

    pub fn reset(&mut self) {
        self.free = [INVALID_PFN; MAX_ORDER + 1];
        self.free_pages = 0;

        // важно: не трогай весь диапазон, если он огромный.
        // reset должен вызываться на зоне, которую ты реально собираешь.
        // Но ты задаёшь base_pfn/pages ровно под зону, так что ok.
        for i in 0..self.pages {
            let pfn = self.base_pfn + i;
            if let Some(f) = get_sparse_memory().pfn_to_frame(pfn) {
                f.tag = BuddyTag::Unused;
                f.order = 0;
                f.next_free = INVALID_PFN;
                f.prev_free = INVALID_PFN;
            }
        }
    }

    // -----------------------------
    // boot-init fast path
    // -----------------------------

    /// Быстро добавить run, который УЖЕ гарантированно Usable (пришёл из PfnRunIter).
    /// Никаких block_all_usable().
    pub fn add_usable_run_fast(&mut self, start: Pfn, len: usize) {
        if len == 0 || self.pages == 0 {
            return;
        }

        // clamp к зоне
        let zone_start = self.base_pfn;
        let zone_end = self.base_pfn + self.pages;

        let mut p = start;
        let mut remaining = len;

        if p < zone_start {
            let drop = zone_start - p;
            if drop >= remaining { return; }
            p += drop;
            remaining -= drop;
        }
        if p >= zone_end {
            return;
        }
        let max_len = zone_end - p;
        if remaining > max_len {
            remaining = max_len;
        }

        while remaining > 0 {
            let mut order = max_block_order_for(p, remaining);
            if order > self.max_order {
                order = self.max_order;
            }

            // ВАЖНО: здесь мы НЕ понижаем order проверками usable.
            // Run гарантированно usable.
            self.insert_free_block_boot(p, order);

            let step = 1usize << order;
            p += step;
            remaining -= step;
        }
    }

    #[inline]
    fn insert_free_block_boot(&mut self, head: Pfn, order: usize) {
        debug_assert!(order <= self.max_order);
        debug_assert!(self.in_range(head));

        // head
        {
            let fh = self.frame_mut(head);
            debug_assert!(fh.state == FrameState::Usable);
            fh.tag = BuddyTag::Free;
            fh.order = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        // остальные страницы блока — не-head
        let n = 1usize << order;
        for i in 1..n {
            let p = head + i;
            // В boot-init это допустимо: если sparse пометил их usable, мы их “занимаем” в блок.
            let f = self.frame_mut(p);
            debug_assert!(f.state == FrameState::Usable);
            f.tag = BuddyTag::Unused;
            f.order = 0;
            f.prev_free = INVALID_PFN;
            f.next_free = INVALID_PFN;
        }

        self.push_free(head, order);
        self.free_pages += n;

        self.try_coalesce_up_boot(head, order);
    }

    fn try_coalesce_up_boot(&mut self, mut head: Pfn, mut order: usize) {
        while order < self.max_order {
            let buddy = self.buddy_of(head, order);

            // Ключевой момент: никаких per-page проверок.
            // Если buddy не free-head нужного порядка — merge невозможен.
            if !self.is_free_head(buddy, order) {
                break;
            }

            // удалить оба
            let _ = self.remove_free(head, order);
            let _ = self.remove_free(buddy, order);

            // merged head
            let a = self.local(head);
            let b = self.local(buddy);
            head = if a <= b { head } else { buddy };
            order += 1;

            {
                let fh = self.frame_mut(head);
                fh.tag = BuddyTag::Free;
                fh.order = order as u8;
                fh.prev_free = INVALID_PFN;
                fh.next_free = INVALID_PFN;
            }

            self.push_free(head, order);
            // free_pages не меняется при merge (n+n -> 2n)
        }
    }

    // -----------------------------
    // free list primitives
    // -----------------------------

    fn push_free(&mut self, head: Pfn, order: usize) {
        let old = self.free[order];

        {
            let fh = self.frame_mut(head);
            fh.tag = BuddyTag::Free;
            fh.order = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = old;
        }

        if old != INVALID_PFN {
            self.frame_mut(old).prev_free = head;
        }

        self.free[order] = head;
    }

    fn pop_free(&mut self, order: usize) -> Option<Pfn> {
        let head = self.free[order];
        if head == INVALID_PFN {
            return None;
        }

        let next = self.frame_mut(head).next_free;
        self.free[order] = next;

        if next != INVALID_PFN {
            self.frame_mut(next).prev_free = INVALID_PFN;
        }

        {
            let fh = self.frame_mut(head);
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        Some(head)
    }

    fn remove_free(&mut self, head: Pfn, order: usize) -> bool {
        let fh = self.frame_mut(head);
        debug_assert!(fh.tag == BuddyTag::Free && fh.order as usize == order);

        let prev = fh.prev_free;
        let next = fh.next_free;

        if prev != INVALID_PFN {
            self.frame_mut(prev).next_free = next;
        } else {
            self.free[order] = next;
        }

        if next != INVALID_PFN {
            self.frame_mut(next).prev_free = prev;
        }

        {
            let fh = self.frame_mut(head);
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        true
    }

    // -----------------------------
    // runtime alloc/free
    // -----------------------------

    pub fn alloc(&mut self, order: usize) -> Option<Pfn> {
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

        let head = self.pop_free(cur)?;

        // мы забираем блок размера 2^cur
        self.free_pages = self.free_pages.saturating_sub(self.order_to_pages(cur));

        // split вниз до нужного order
        let mut o = cur;
        while o > order {
            o -= 1;
            let buddy = self.buddy_of(head, o);

            // buddy должен быть usable — но проверка per-page не нужна
            debug_assert!(self.is_usable(buddy));

            {
                let fb = self.frame_mut(buddy);
                fb.tag = BuddyTag::Free;
                fb.order = o as u8;
                fb.prev_free = INVALID_PFN;
                fb.next_free = INVALID_PFN;
            }
            self.push_free(buddy, o);

            // мы вернули половину блока обратно
            self.free_pages += self.order_to_pages(o);
        }

        {
            let fh = self.frame_mut(head);
            debug_assert!(fh.state == FrameState::Usable);
            fh.tag = BuddyTag::Allocated;
            fh.order = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        // внутренние страницы пометим как Unused
        let n = 1usize << order;
        for i in 1..n {
            let p = head + i;
            if !self.is_usable(p) { break; }
            let f = self.frame_mut(p);
            f.tag = BuddyTag::Unused;
            f.order = 0;
            f.prev_free = INVALID_PFN;
            f.next_free = INVALID_PFN;
        }

        Some(head)
    }

    pub fn free(&mut self, head: Pfn) {
        debug_assert!(self.in_range(head));
        debug_assert!(self.is_usable(head));

        let order = {
            let fh = self.frame_mut(head);
            debug_assert!(fh.tag == BuddyTag::Allocated, "buddy.free: not allocated head");
            fh.order as usize
        };

        {
            let fh = self.frame_mut(head);
            fh.tag = BuddyTag::Free;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        // коалес по buddy-head, без block_all_usable
        let merged = self.free_block_coalesce_fast(head, order);
        let final_order = self.frame_mut(merged).order as usize;

        self.push_free(merged, final_order);
        self.free_pages += self.order_to_pages(final_order);
    }

    fn free_block_coalesce_fast(&mut self, mut head: Pfn, mut order: usize) -> Pfn {
        while order < self.max_order {
            let buddy = self.buddy_of(head, order);

            if !self.is_free_head(buddy, order) {
                break;
            }

            // remove buddy из freelist; head ещё не в freelist (мы не пушили)
            if !self.remove_free(buddy, order) {
                break;
            }

            // merge -> min(local)
            let a = self.local(head);
            let b = self.local(buddy);
            head = if a <= b { head } else { buddy };
            order += 1;

            let fh = self.frame_mut(head);
            fh.tag = BuddyTag::Free;
            fh.order = order as u8;
            fh.prev_free = INVALID_PFN;
            fh.next_free = INVALID_PFN;
        }

        // пометить внутренние страницы merged блока как Unused
        let n = 1usize << order;
        for i in 1..n {
            let p = head + i;
            if !self.is_usable(p) { break; }
            let f = self.frame_mut(p);
            f.tag = BuddyTag::Unused;
            f.order = 0;
            f.prev_free = INVALID_PFN;
            f.next_free = INVALID_PFN;
        }

        head
    }
}
