use limine::memory_map::Entry;
use crate::{arch::amd64::memory::{mem_subsys_tests::selftest_all_memory_subsystem, pmm::init_physical_memory, vmm::init_virtual_memory}, early_println};
pub mod misc;
pub mod pmm;
pub mod vmm;
mod mem_subsys_tests;

pub struct MemoryInitInfo<'a> {
    pub hhdm_offset: u64,
    pub memmap_entry: &'a[&'a Entry]
}

pub fn init_memory_subsys(init_info: MemoryInitInfo) {
    early_println!("Hhdm offset: {:#018x}", init_info.hhdm_offset);

    early_println!("Initializing physical memory manager...");
    init_physical_memory(init_info.hhdm_offset, init_info.memmap_entry);
    early_println!("Physical memory manager initialized!");

    early_println!("Initializing virtual memory manager...");
    init_virtual_memory(init_info.hhdm_offset);
    early_println!("Virtual memory manager initialized!");

    selftest_all_memory_subsystem();
}