use core::{alloc::{GlobalAlloc, Layout}, ptr::{NonNull, null_mut}};

use bitflags::bitflags;
use x86_64::{PhysAddr, VirtAddr};

use crate::{arch::amd64::memory::{misc::{align_up, pages_to_order, phys_to_virt, virt_to_phys}, pmm::{pages_allocator::{KERNEL_PAGES, SAFE_KERNEL_PAGES, alloc_pages_by_order, free_pages}, slab::{SLAB_MAX_ALLOC, SLAB_MIN_ALIGN, slab_alloc, slab_try_free}, sparsemem::PAGE_SIZE}}};

bitflags! {
    pub struct KmallocFlags: u8 {
        const Zeroed = 1 << 0;
    }
}

pub fn kmalloc(bytes: usize, flags: KmallocFlags) -> Option<VirtAddr> {
    if bytes == 0 {
        return None;
    }

    let zeroed = flags.contains(KmallocFlags::Zeroed);

    if bytes <= SLAB_MAX_ALLOC {
        if let Some(p) = slab_alloc(bytes, zeroed) {
            return Some(p);
        }
    }

    let bytes = align_up(bytes, PAGE_SIZE);
    let pages = bytes / PAGE_SIZE;
    let order = pages_to_order(pages);

    let flags = if zeroed {
        SAFE_KERNEL_PAGES
    } else {
        KERNEL_PAGES
    };

    let phys = alloc_pages_by_order(order, flags)?;
    let virt = phys_to_virt(phys.as_u64() as usize);
    Some(VirtAddr::new(virt as u64))
}

pub fn kfree(ptr: VirtAddr) {
    if ptr.is_null() {
        return;
    }

    if slab_try_free(ptr) {
        return;
    }

    let phys = virt_to_phys(ptr.as_u64() as usize);

    free_pages(PhysAddr::new(phys as u64));
}

unsafe fn alloc_for_layout(layout: Layout, zeroed: bool) -> *mut u8 {
    let size = layout.size();
    let align = layout.align();

    if size == 0 {
        return NonNull::<u8>::dangling().as_ptr();
    }

    if size <= SLAB_MAX_ALLOC && align <= SLAB_MIN_ALIGN {
        let flags = if zeroed { KmallocFlags::Zeroed } else { KmallocFlags::empty() };
        if let Some(va) = kmalloc(size, flags) {
            return va.as_mut_ptr::<u8>();
        }
        return null_mut();
    }

    let size_rounded = align_up(size, PAGE_SIZE);
    let pages_for_size = size_rounded / PAGE_SIZE;

    let align_rounded = align_up(align, PAGE_SIZE);
    let pages_for_align = core::cmp::max(1, align_rounded / PAGE_SIZE);

    let order_size = pages_to_order(pages_for_size);
    let order_align = pages_to_order(pages_for_align);
    let order = core::cmp::max(order_size, order_align);

    let pflags = if zeroed { SAFE_KERNEL_PAGES } else { KERNEL_PAGES };

    let phys = match alloc_pages_by_order(order, pflags) {
        Some(p) => p,
        None => return null_mut(),
    };

    let virt = phys_to_virt(phys.as_u64() as usize);
    virt as *mut u8
}

struct PhysicalAllocator;

unsafe impl GlobalAlloc for PhysicalAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        unsafe { alloc_for_layout(layout, false) }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if layout.size() == 0 {
            return;
        }
        if ptr.is_null() {
            return;
        }
        kfree(VirtAddr::new(ptr as u64));
    }

    unsafe fn alloc_zeroed(&self, layout: Layout) -> *mut u8 {
        unsafe { alloc_for_layout(layout, true) }
    }

    unsafe fn realloc(&self, ptr: *mut u8, old_layout: Layout, new_size: usize) -> *mut u8 {
        unsafe {
            if old_layout.size() == 0 {
                let new_layout = Layout::from_size_align_unchecked(new_size, old_layout.align());
                return self.alloc(new_layout);
            }
            
            if new_size == 0 {
                self.dealloc(ptr, old_layout);
                return NonNull::<u8>::dangling().as_ptr();
            }

            let new_layout = Layout::from_size_align_unchecked(new_size, old_layout.align());
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
static GLOBAL_ALLOC: PhysicalAllocator = PhysicalAllocator;