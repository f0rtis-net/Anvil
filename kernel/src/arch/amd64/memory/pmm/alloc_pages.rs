use core::intrinsics::write_bytes;

use crate::arch::amd64::memory::{misc::{phys_to_virt, virt_to_phys}, pmm::{HHDM_OFFSET, frame_area::{FRAME_SIZE, FramesDB, frames_db, pfn_to_physical, physical_to_pfn}, mem_zones::MemoryZoneType, zones_manager::zones_manager}};

pub fn alloc_pages(order: usize, zone: MemoryZoneType, zeroed: bool) -> Option<usize> {
    let start_pfn = {
        let mut mngr = zones_manager();
        let z = match zone {
            MemoryZoneType::High => mngr.get_highmem_zone(),
            MemoryZoneType::Dma => mngr.get_dma_zone()
        };

        z.alloc_order(order)?
    };

    let phys = pfn_to_physical(start_pfn);
    let virt = unsafe {phys_to_virt(HHDM_OFFSET, phys)};

    if zeroed {
        let bytes = (1usize << order) * FRAME_SIZE;
        unsafe { write_bytes(virt as *mut u8, 0, bytes) };
    }

    Some(virt)
}

pub fn free_pages(virt: usize) {
    if virt == 0 { return; }

    let phys = virt_to_phys(unsafe { HHDM_OFFSET }, virt);
    let pfn  = physical_to_pfn(phys);

    let zone = {
        frames_db().frame(pfn).zone
    };

    let mut mgr = zones_manager();
    match zone {
        MemoryZoneType::High => mgr.get_highmem_zone().free(pfn),
        MemoryZoneType::Dma  => mgr.get_dma_zone().free(pfn),
    }
}