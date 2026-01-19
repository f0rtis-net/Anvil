use limine::memory_map::Entry;
use crate::{arch::amd64::memory::{pmm::init_physical_memory, test::test_all_memory_subsystem, vmm::init_virtual_memory}, serial_println};
pub mod misc;
pub mod pmm;
pub mod vmm;
mod test;

pub struct MemoryInitInfo<'a> {
    pub hhdm_offset: u64,
    pub memmap_entry: &'a[&'a Entry]
}

pub fn init_memory_subsys(init_info: MemoryInitInfo) {
    serial_println!("Hhdm offset: {:#018x}", init_info.hhdm_offset);

    serial_println!("Initializing physical memory manager...");
    init_physical_memory(init_info.hhdm_offset, init_info.memmap_entry);
    serial_println!(" Physical memory manager initialized!");

    serial_println!("Initializing virtual memory manager...");
    init_virtual_memory(init_info.hhdm_offset);
    serial_println!("Virtual memory manager initialized!");

    test_all_memory_subsystem();
}