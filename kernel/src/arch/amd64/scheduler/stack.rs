#![allow(dead_code)]

use alloc::vec::Vec;
use x86_64::{PhysAddr, VirtAddr, structures::paging::PageTableFlags};

use crate::arch::amd64::memory::{
    pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order, free_pages},
    vmm::{PAGE_SIZE, map_single_page, unmap_single_page, kernel_pt},
};

pub const DEFAULT_KERNEL_STACK_SIZE: usize = 16 * PAGE_SIZE;

const KERNEL_STACKS_VA_BASE: u64 = 0xFFFF_C000_0000_0000;
const KERNEL_STACKS_VA_SIZE: u64 = 256 * 1024 * 1024 * 1024; 

static STACK_VA_BUMP: core::sync::atomic::AtomicU64 =
    core::sync::atomic::AtomicU64::new(KERNEL_STACKS_VA_BASE);

fn reserve_stack_va(pages: usize) -> VirtAddr {
    let size = (pages * PAGE_SIZE) as u64;
    let base = STACK_VA_BUMP.fetch_add(
        size,
        core::sync::atomic::Ordering::Relaxed,
    );
    assert!(
        base + size <= KERNEL_STACKS_VA_BASE + KERNEL_STACKS_VA_SIZE,
        "reserve_stack_va: kernel stack VA space exhausted"
    );
    VirtAddr::new(base)
}

pub struct KernelStack {
    pub bottom:     VirtAddr,
    pub top:        VirtAddr,
    pages:          Vec<PhysAddr>,
}

impl KernelStack {
    #[inline]
    pub fn guard_page_va(&self) -> VirtAddr {
        self.bottom - PAGE_SIZE as u64
    }
}

pub fn allocate_kernel_stack(size: usize) -> KernelStack {
    assert!(
        size > 0 && size % PAGE_SIZE == 0,
        "allocate_kernel_stack: size must be page-aligned and non-zero, got {}",
        size
    );

    let page_count = size / PAGE_SIZE;

    let mut phys_pages: Vec<PhysAddr> = Vec::with_capacity(page_count);
    for i in 0..page_count {
        let phys = alloc_pages_by_order(0, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
            .unwrap_or_else(|| {
                for p in &phys_pages {
                    free_pages(*p);
                }
                panic!("allocate_kernel_stack: OOM at page {}/{}", i, page_count);
            });
        phys_pages.push(phys);
    }

    let va_base   = reserve_stack_va(1 + page_count);                    
    let stack_va  = va_base + PAGE_SIZE as u64;      

    let flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;

    {
        let mut pt = kernel_pt().lock();
        for (i, &phys) in phys_pages.iter().enumerate() {
            let virt = stack_va + (i * PAGE_SIZE) as u64;
            if let Err(e) = map_single_page(&mut pt, virt, phys, flags) {
                for j in 0..i {
                    let v = stack_va + (j * PAGE_SIZE) as u64;
                    let _ = unmap_single_page(&mut pt, v);
                }
                drop(pt);
                for p in &phys_pages {
                    free_pages(*p);
                }
                panic!(
                    "allocate_kernel_stack: map_single_page failed at va={:#x}: {}",
                    virt.as_u64(), e
                );
            }
        }
    }

    KernelStack {
        bottom: stack_va,
        top:    stack_va + size as u64,
        pages:  phys_pages,
    }
}

pub fn deallocate_kernel_stack(stack: KernelStack) {
    let mut pt = kernel_pt().lock();
    for (i, phys) in stack.pages.iter().enumerate() {
        let virt = stack.bottom + (i * PAGE_SIZE) as u64;
        unmap_single_page(&mut pt, virt)
            .unwrap_or_else(|e| {
                panic!(
                    "deallocate_kernel_stack: unmap failed at va={:#x}: {}",
                    virt.as_u64(), e
                )
            });
        free_pages(*phys);
    }
}