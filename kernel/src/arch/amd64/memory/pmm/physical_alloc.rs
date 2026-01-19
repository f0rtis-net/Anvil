use bitflags::bitflags;
use x86_64::{PhysAddr, VirtAddr};

use crate::arch::amd64::memory::{misc::{align_up, pages_to_order, phys_to_virt, virt_to_phys}, pmm::{HHDM_OFFSET, pages_allocator::{KERNEL_PAGES, SAFE_KERNEL_PAGES, alloc_pages_by_order, free_pages}, slab::{SLAB_MAX_ALLOC, slab_alloc, slab_try_free}, sparsemem::PAGE_SIZE}};

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
    let virt = phys_to_virt(unsafe { HHDM_OFFSET }, phys.as_u64() as usize);
    Some(VirtAddr::new(virt as u64))
}

pub fn kfree(ptr: VirtAddr) {
    if ptr.is_null() {
        return;
    }

    if slab_try_free(ptr) {
        return;
    }

    let phys = virt_to_phys(unsafe { HHDM_OFFSET }, ptr.as_u64() as usize);

    free_pages(PhysAddr::new(phys as u64));
}