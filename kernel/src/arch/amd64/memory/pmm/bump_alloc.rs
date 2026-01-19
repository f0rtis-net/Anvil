use core::ptr::NonNull;

use crate::arch::amd64::memory::misc::align_up;

#[derive(Debug, Clone, Copy)]
pub struct BumpState {
    end: usize,
    next: usize,
}

impl BumpState {
    pub const fn init(start: usize, end: usize) -> Self {
        debug_assert!(start < end);
        Self { 
            end: end, 
            next: start 
        }
    }

    pub fn alloc(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        if size == 0 {
            return None;
        }

        let align = align.max(1);
        if !align.is_power_of_two() {
            return None;
        }

        let aligned = align_up(self.next, align);

        let new_next = aligned.checked_add(size)?;
        if new_next > self.end {
            return None;
        }

        self.next = new_next;
        NonNull::new(aligned as *mut u8)
    }

    pub fn alloc_zeroed(&mut self, size: usize, align: usize) -> Option<NonNull<u8>> {
        let p = self.alloc(size, align)?;
        unsafe {
            core::ptr::write_bytes(p.as_ptr(), 0, size);
        }
        Some(p)
    }
}
