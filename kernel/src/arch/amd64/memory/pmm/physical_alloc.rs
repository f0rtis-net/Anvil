#![allow(dead_code)]

use core::{
    alloc::{GlobalAlloc, Layout},
    ptr::{NonNull, null_mut},
};

use bitflags::bitflags;
use x86_64::{PhysAddr, VirtAddr};

use crate::arch::amd64::memory::{
    misc::{align_up, pages_to_order, phys_to_virt, virt_to_phys},
    pmm::{
        pages_allocator::{KERNEL_PAGES, SAFE_KERNEL_PAGES, alloc_pages_by_order, free_pages},
        slab::{SLAB_MAX_ALLOC, SLAB_MIN_ALIGN, slab_alloc, slab_free},
        sparsemem::PAGE_SIZE,
    },
};

bitflags! {
    pub struct KmallocFlags: u8 {
        const ZEROED = 1 << 0;
        const KERNEL = 1 << 1;
    }
}

pub fn kmalloc(bytes: usize, flags: KmallocFlags) -> Option<VirtAddr> {
    if bytes == 0 {
        return None;
    }

    let zeroed = flags.contains(KmallocFlags::ZEROED);

    if bytes <= SLAB_MAX_ALLOC {
        return slab_alloc(bytes, zeroed);
    }

    let aligned = align_up(bytes, PAGE_SIZE);
    let order   = pages_to_order(aligned / PAGE_SIZE);
    let pflags  = if zeroed { SAFE_KERNEL_PAGES } else { KERNEL_PAGES };

    let phys = alloc_pages_by_order(order, pflags)?;
    let virt = phys_to_virt(phys.as_u64() as usize);

    Some(VirtAddr::new(virt as u64))
}

pub fn kfree(ptr: VirtAddr) {
    if ptr.as_u64() == 0 {
        return;
    }

    if slab_free(ptr) {
        return;
    }

    let phys = virt_to_phys(ptr.as_u64() as usize);
    free_pages(PhysAddr::new(phys as u64));
}

unsafe fn alloc_for_layout(layout: Layout, zeroed: bool) -> *mut u8 {
    let size  = layout.size();
    let align = layout.align();

    if size == 0 {
        return NonNull::<u8>::dangling().as_ptr();
    }

    if size <= SLAB_MAX_ALLOC && align <= SLAB_MIN_ALIGN {
        return match slab_alloc(size, zeroed) {
            Some(va) => va.as_mut_ptr::<u8>(),
            None     => null_mut(),
        };
    }

    let size_pages  = align_up(size,  PAGE_SIZE) / PAGE_SIZE;
    let align_pages = align_up(align, PAGE_SIZE) / PAGE_SIZE;

    let order = core::cmp::max(
        pages_to_order(size_pages),
        pages_to_order(core::cmp::max(1, align_pages)),
    );

    let pflags = if zeroed { SAFE_KERNEL_PAGES } else { KERNEL_PAGES };

    match alloc_pages_by_order(order, pflags) {
        Some(phys) => phys_to_virt(phys.as_u64() as usize) as *mut u8,
        None       => null_mut(),
    }
}

struct KernelAllocator;

unsafe impl GlobalAlloc for KernelAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { alloc_for_layout(layout, false) }
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { alloc_for_layout(layout, true) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 || ptr.is_null() {
            return;
        }
        kfree(VirtAddr::new(ptr as u64));
    }

    unsafe fn realloc(
        &self,
        ptr:        *mut u8,
        old_layout: Layout,
        new_size:   usize,
    ) -> *mut u8 {
        unsafe {
            if old_layout.size() == 0 {
                let new_layout = Layout::from_size_align_unchecked(new_size, old_layout.align());
                return self.alloc(new_layout);
            }
            if new_size == 0 {
                self.dealloc(ptr, old_layout);
                return NonNull::<u8>::dangling().as_ptr();
            }

            let new_layout =
                Layout::from_size_align_unchecked(new_size, old_layout.align());
            let new_ptr = self.alloc(new_layout);
            if new_ptr.is_null() {
                return null_mut();
            }

            let to_copy = core::cmp::min(old_layout.size(), new_size);
            core::ptr::copy_nonoverlapping(ptr, new_ptr, to_copy);

            self.dealloc(ptr, old_layout);
            new_ptr
        }
    }
}

#[global_allocator]
static GLOBAL_ALLOC: KernelAllocator = KernelAllocator;