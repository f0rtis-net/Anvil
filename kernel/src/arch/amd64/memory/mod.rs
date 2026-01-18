use limine::memory_map::Entry;
use crate::arch::amd64::memory::pmm::init_physical_memory;
pub mod misc;
pub mod pmm;
mod vmm;

pub struct MemoryInitInfo<'a> {
    pub hhdm_offset: u64,
    pub memmap_entry: &'a[&'a Entry]
}

pub fn init_memory_subsys(init_info: MemoryInitInfo) {
    init_physical_memory(init_info.hhdm_offset, init_info.memmap_entry);
}