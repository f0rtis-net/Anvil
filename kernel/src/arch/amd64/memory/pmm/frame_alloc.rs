use core::array;

use alloc::vec::Vec;

use crate::arch::amd64::memory::misc::{align_down, align_up, floor_log2};

pub const PAGE_SIZE: usize = 4096;
pub const PAGE_SHIFT: usize = 12;
const MAX_ORDER: usize = 11;

type Pfn = usize;

const META_UNUSED: u8 = 0xFF;
const META_ALLOCATED: u8 = 0x00; // lower bits = order
const META_FREE: u8 = 0x80;      // high bit = free head
const META_ORDER_MASK: u8 = 0x7F;

pub struct Buddy {
    base_pfn: Pfn,
    page_count: usize,
    max_order: usize,

    // free lists store PFN heads
    free: [Vec<Pfn>; MAX_ORDER + 1],

    // meta indexed by pfn-local index
    page_meta: Vec<u8>,

    heap_calculated: bool
}


impl Buddy {
    pub fn empty() -> Self {
        Self {
            base_pfn: 0,
            page_count: 0,
            max_order: 0,
            free: array::from_fn(|_| Vec::new()),
            page_meta: Vec::new(),
            heap_calculated: false
        }
    }
    
    pub fn init(&mut self, phys_start: usize, phys_end: usize) {
        let start = align_up(phys_start, PAGE_SIZE);
        let end   = align_down(phys_end, PAGE_SIZE);

        let pages = (end - start) / PAGE_SIZE;

        self.base_pfn = start >> PAGE_SHIFT;
        self.page_count = pages;
        self.max_order = core::cmp::min(floor_log2(pages), MAX_ORDER);

        self.page_meta = alloc::vec![META_UNUSED; pages];

        self.free = core::array::from_fn(|_| Vec::new());

        let mut idx = 0;
        while idx < pages {
            let remain = pages - idx;
            let mut order = floor_log2(remain).min(self.max_order);

            while order > 0 && (idx & ((1 << order) - 1)) != 0 {
                order -= 1;
            }

            self.push_free(idx, order);
            idx += 1 << order;
        }
    }

    pub fn alloc_pfn(&mut self) -> Option<Pfn> {
        self.alloc_order(0)
    }

    pub fn alloc_pfns(&mut self, order: usize) -> Option<Pfn> {
        self.alloc_order(order)
    }

    pub fn free_pfn(&mut self, pfn: Pfn) {
        if pfn < self.base_pfn {
            return;
        }
        let idx = pfn - self.base_pfn;
        if idx >= self.page_count {
            return;
        }
        self.free_idx(idx);
    }

    fn alloc_order(&mut self, order: usize) -> Option<Pfn> {
        if order > self.max_order {
            return None;
        }

        let mut cur = order;
        while cur <= self.max_order && self.free[cur].is_empty() {
            cur += 1;
        }
        if cur > self.max_order {
            return None;
        }

        let idx = self.free[cur].pop().unwrap();

        while cur > order {
            cur -= 1;
            let buddy = idx ^ (1 << cur);
            self.push_free(buddy, cur);
        }

        self.page_meta[idx] = META_ALLOCATED | (order as u8);
        Some(self.base_pfn + idx)
    }

    fn free_idx(&mut self, mut idx: usize) {
        let meta = self.page_meta[idx];
        if meta == META_UNUSED || (meta & META_FREE) != 0 {
            return;
        }

        let mut order = (meta & META_ORDER_MASK) as usize;
        self.page_meta[idx] = META_UNUSED;

        while order < self.max_order {
            let buddy = idx ^ (1 << order);
            if buddy >= self.page_count {
                break;
            }

            if !self.is_free_head(buddy, order) {
                break;
            }

            self.remove_free(buddy, order);
            self.page_meta[buddy] = META_UNUSED;

            idx = idx.min(buddy);
            order += 1;
        }

        self.push_free(idx, order);
    }

    fn push_free(&mut self, idx: usize, order: usize) {
        self.page_meta[idx] = META_FREE | (order as u8);
        self.free[order].push(idx);
    }

    fn remove_free(&mut self, idx: usize, order: usize) {
        if let Some(pos) = self.free[order].iter().position(|&x| x == idx) {
            self.free[order].swap_remove(pos);
        }
    }

    fn is_free_head(&self, idx: usize, order: usize) -> bool {
        let m = self.page_meta[idx];
        (m & META_FREE) != 0 && (m & META_ORDER_MASK) as usize == order
    }
}

impl Buddy {
    pub fn calculate_needed_heap(phys_start: usize, phys_end: usize) -> usize {
        let start = align_up(phys_start, PAGE_SIZE);
        let end   = align_down(phys_end, PAGE_SIZE);

        if end <= start {
            return 0;
        }

        let pages = (end - start) / PAGE_SIZE;

        let page_meta_bytes = pages * core::mem::size_of::<u8>();
        let free_list_bytes = pages * core::mem::size_of::<Pfn>();

        let overhead = 512; //overhead

        page_meta_bytes + free_list_bytes + overhead
    }
}

#[inline]
pub fn pfn_to_physical(pfn: usize) -> usize {
    return pfn << PAGE_SHIFT;
}