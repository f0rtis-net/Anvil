#![allow(dead_code)]

use bitflags::bitflags;
use x86_64::PhysAddr;

use crate::arch::amd64::memory::{misc::phys_to_virt, pmm::{
    sparsemem::{PAGE_SHIFT, PAGE_SIZE, Pfn},
    zones_manager::{ZoneId, get_zones_manager},
}};

bitflags! {
    pub struct PAllocFlags: u32 {
        const KERNEL = 1 << 0;
        const DMA    = 1 << 1;
        const ZEROED = 1 << 2;
    }
}

pub const KERNEL_PAGES: PAllocFlags = PAllocFlags::KERNEL;

pub const SAFE_KERNEL_PAGES: PAllocFlags =
    PAllocFlags::from_bits_truncate(PAllocFlags::KERNEL.bits() | PAllocFlags::ZEROED.bits());

fn flags_to_zone(flags: &PAllocFlags) -> ZoneId {
    let kernel = flags.contains(PAllocFlags::KERNEL);
    let dma    = flags.contains(PAllocFlags::DMA);

    match (kernel, dma) {
        (true,  false) => ZoneId::High,
        (false, true)  => ZoneId::Dma,
        (true,  true)  => panic!("PAllocFlags: KERNEL and DMA are mutually exclusive"),
        (false, false) => panic!("PAllocFlags: no zone flag specified (need KERNEL or DMA)"),
    }
}

#[inline]
pub fn alloc_physical_frame_pfn() -> Option<Pfn> {
    get_zones_manager()
        .lock()
        .alloc_pages(ZoneId::High, 0)
}

pub fn alloc_pages_by_order(order: usize, flags: PAllocFlags) -> Option<PhysAddr> {
    let zone   = flags_to_zone(&flags);
    let zeroed = flags.contains(PAllocFlags::ZEROED);

    let pfn = get_zones_manager()
        .lock()
        .alloc_pages(zone, order)?;

    let phys = pfn << PAGE_SHIFT;

    if zeroed {
        let virt  = phys_to_virt(phys);
        let pages = 1usize << order;
        unsafe {
            core::ptr::write_bytes(virt as *mut u8, 0, pages * PAGE_SIZE);
        }
    }

    Some(PhysAddr::new(phys as u64))
}

pub fn free_pages(ptr: PhysAddr) {
    debug_assert!(
        ptr.as_u64() % PAGE_SIZE as u64 == 0,
        "free_pages: address {:#x} is not page-aligned",
        ptr.as_u64()
    );

    let pfn = (ptr.as_u64() as usize) >> PAGE_SHIFT;

    get_zones_manager()
        .lock()
        .free_pages(pfn);
}