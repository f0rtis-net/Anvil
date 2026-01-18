use core::intrinsics::write_bytes;
use limine::memory_map::Entry;
use spin::{Mutex, Once};

use crate::arch::amd64::memory::pmm::tests::test_pmm_all;
use crate::arch::amd64::memory::pmm::zones_manager::{MemZonesManager, init_memory_zones};
use crate::{
    arch::amd64::memory::{
        misc::{phys_to_virt, virt_to_phys},
        pmm::{
            frame_alloc::pages_to_order,
            frame_area::{FRAME_SIZE, pfn_to_physical, physical_to_pfn},
        },
    },
    serial_println,
};

mod frame_alloc;
mod frame_area;
mod memblock;
mod mem_zones;
mod early_allocator;
mod zones_manager;
mod tests;

static ZONES_MANAGER: Once<Mutex<MemZonesManager>> = Once::new();
static mut HHDM_OFFSET: usize = 0;

bitflags::bitflags! {
    pub struct KmallocFlags: u32 {
        const Kernel = 1 << 0;
        const Zeroed = 1 << 2;
    }
}

pub fn init_physical_memory(hhdm_offset: u64, mmap: &[&Entry]) {
    unsafe { HHDM_OFFSET = hhdm_offset as usize; }

    serial_println!("Initializing physical memory manager...");

    ZONES_MANAGER.call_once(|| {
        Mutex::new(init_memory_zones(unsafe {HHDM_OFFSET}, mmap))
    }); 

    test_pmm_all();

    serial_println!("Physical memory manager initialized!");
}

pub fn kmalloc(bytes: usize, flags: KmallocFlags) -> usize {
    if bytes == 0 {
        return 0;
    }

    let pages = (bytes + FRAME_SIZE - 1) / FRAME_SIZE;
    let order = pages_to_order(pages);

    let start_pfn = {
        let mut zone_manager = ZONES_MANAGER
            .get()
            .expect("PMM not initialized")
            .lock();

        zone_manager.get_highmem_zone()
            .alloc_order(order)
            .expect("kmalloc: OOM")
    };

    let phys = pfn_to_physical(start_pfn);
    let virt = unsafe { phys_to_virt(HHDM_OFFSET, phys) };

    if flags.contains(KmallocFlags::Zeroed) {
        let alloc_bytes = (1usize << order) * FRAME_SIZE;
        unsafe {
            core::ptr::write_bytes(virt as *mut u8, 0, alloc_bytes);
        }
    }

    virt
}


pub fn kfree(ptr: usize) {
    if ptr == 0 {
        return;
    }

    let phys = unsafe { virt_to_phys(HHDM_OFFSET, ptr) };
    let pfn = physical_to_pfn(phys);

    let mut mgr = ZONES_MANAGER
        .get()
        .expect("PMM not initialized")
        .lock();

    mgr.get_highmem_zone().free(pfn);
}