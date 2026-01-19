use bitflags::bitflags;
use x86_64::PhysAddr;

use crate::arch::amd64::memory::{pmm::{HHDM_OFFSET, sparsemem::{PAGE_SHIFT, PAGE_SIZE, Pfn}, zones_manager::{ZoneId, get_zones_manager}}};

bitflags! {
    pub struct PAllocFlags: u32 {
        const Kernel = 1 << 0;
        const Dma    = 1 << 1;
        const Zeroed = 1 << 2;
    }
}

pub const SAFE_KERNEL_PAGES: PAllocFlags =
    PAllocFlags::from_bits_truncate(
        PAllocFlags::Kernel.bits() | PAllocFlags::Zeroed.bits()
    );

pub const KERNEL_PAGES: PAllocFlags = PAllocFlags::Kernel;

fn flags_to_zone(flags: &PAllocFlags) -> ZoneId {
    if flags.contains(PAllocFlags::Kernel) && flags.contains(PAllocFlags::Dma) {
        panic!("Zone flag must be 1!");
    }

    if flags.contains(PAllocFlags::Kernel) {
        ZoneId::High
    } else {
        ZoneId::Dma
    }
}

pub fn alloc_physical_frame_pfn() -> Option<Pfn> {
    get_zones_manager()
        .lock()
        .alloc_pages(ZoneId::High, 0)
}

pub fn alloc_pages_by_order(order: usize, flags: PAllocFlags) -> Option<PhysAddr> {
    let zone = flags_to_zone(&flags);
    let zeroed = flags.contains(PAllocFlags::Zeroed);

    let real_pages = 1usize << order;

    let pfn = get_zones_manager()
        .lock()
        .alloc_pages(zone, order)?;

    let phys_addr = pfn << PAGE_SHIFT;

    if zeroed {
        let virt_addr = phys_addr + unsafe { HHDM_OFFSET };
        unsafe {
            core::ptr::write_bytes(
                virt_addr as *mut u8,
                0,
                real_pages * PAGE_SIZE,
            );
        }
    }

    Some(PhysAddr::new(phys_addr as u64))
}

pub fn free_pages(ptr: PhysAddr) {
    let pfn = ptr.as_u64() >> PAGE_SHIFT;

    get_zones_manager()
        .lock()
        .free_pages(pfn as usize);
}