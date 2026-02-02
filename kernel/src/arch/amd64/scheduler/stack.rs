use x86_64::{PhysAddr, VirtAddr};

use crate::arch::amd64::memory::{misc::{pages_to_order, phys_to_virt, virt_to_phys}, pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order, free_pages}, vmm::{PAGE_SIZE, kunmap_page}};

pub const DEFAULT_KERNEL_STACK_SIZE: usize = 16 * PAGE_SIZE;

pub struct KernelStack {
    pub bottom: VirtAddr,
    pub top: VirtAddr
}

pub fn allocate_kernel_stack(size: usize) -> KernelStack {
    let pages = size / PAGE_SIZE;
    let order = pages_to_order(pages + 1); // + guard

    let phys = alloc_pages_by_order(order, PAllocFlags::Zeroed)
        .expect("stack alloc failed");

    let virt_base = phys_to_virt(phys.as_u64() as usize);

    let guard_page = virt_base as u64;
    kunmap_page(VirtAddr::new(guard_page)); // making guard page

    let bottom = virt_base + PAGE_SIZE;
    let top = bottom + size;

    KernelStack {
        bottom: VirtAddr::new(bottom as u64),
        top: VirtAddr::new(top as u64),
    }
}

pub fn deallocate_kernel_stack(stack: KernelStack) {
    let phys = virt_to_phys(stack.bottom.as_u64() as usize - PAGE_SIZE); // - PAGE_SIZE cuz we have guard page
    free_pages(PhysAddr::new(phys as u64));
}