use limine::memory_map::Entry;
use crate::{arch::amd64::memory::pmm::{memblock::initialize_memblock_from_mm, slab::slab_init, sparsemem::{init_sparsemem_layer}, zones_manager::init_zones_manager}};

mod memblock;
mod sparsemem;
mod bump_alloc;
mod pfn_iterator;
mod buddy;
mod zones_manager;
mod slab;
mod pmm_tests;

pub mod physical_alloc;
pub mod pages_allocator;

pub static mut HHDM_OFFSET: usize = 0;

pub fn init_physical_memory(hhdm_offset: u64, mmap: &[&Entry]) {
    unsafe { HHDM_OFFSET = hhdm_offset as usize; }

    let mut memblock = initialize_memblock_from_mm(mmap).unwrap();

    init_sparsemem_layer(&mut memblock);

    init_zones_manager();

    slab_init();

    #[cfg(feature = "pmm_tests")]
    pmm_tests::pmm_tests::run_all();
}
