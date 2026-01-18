use limine::memory_map::Entry;
use crate::arch::amd64::memory::pmm::slab::slab_init;
use crate::arch::amd64::memory::pmm::tests::test_pmm_all;
use crate::arch::amd64::memory::pmm::zones_manager::init_memory_zones_manager;
use crate::serial_println;

mod frame_alloc;
mod frame_area;
mod memblock;
mod mem_zones;
mod early_allocator;
mod zones_manager;
mod slab;
mod alloc_pages;
mod tests;
pub mod abstract_allocator;

static mut HHDM_OFFSET: usize = 0;

pub fn init_physical_memory(hhdm_offset: u64, mmap: &[&Entry]) {
    unsafe { HHDM_OFFSET = hhdm_offset as usize; }

    serial_println!("Initializing physical memory manager...");

    init_memory_zones_manager(hhdm_offset as usize, mmap);

    serial_println!("Initalizing slab allocator....");
    slab_init();
    serial_println!("Slab allocator initialized!");

    test_pmm_all();

    serial_println!("Physical memory manager initialized!");
}