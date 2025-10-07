use bootloader::bootinfo::MemoryMap;
use x86_64::{structures::paging::OffsetPageTable, VirtAddr};

use crate::{memory::{kernel_allocator::init_heap, vmem::{active_level_4_table, init_vmemory, BootInfoFrameAllocator}}, println};

mod kernel_allocator;
pub mod vmem;

pub struct MemResult<'a> {
    pub frame_alloc: BootInfoFrameAllocator,
    pub mapper: OffsetPageTable<'a>,
    pub phys_mem_offset: VirtAddr,
}

pub static mut PHYS_OFFS: Option<VirtAddr> = None;

pub fn initialize_memory(physical_memory_offset: u64, memory_map: &'static MemoryMap) -> MemResult<'static> {
    let phys_mem_offset = VirtAddr::new(physical_memory_offset);

    unsafe {
        PHYS_OFFS = Some(phys_mem_offset);
    }

    println!("Initializing phys frame allocator...");
    let mut frame_allocator = unsafe { 
        BootInfoFrameAllocator::init(&memory_map)
    };

    println!("Initializing vmem mapper...");
    let mut mapper = unsafe { init_vmemory(phys_mem_offset) };

    println!("Initializing kernel heap...");
    init_heap(&mut mapper, &mut frame_allocator).expect("kernel heap initialization failed");

    println!("Memory module initialized successfully!");
    return MemResult { frame_alloc: frame_allocator, mapper, phys_mem_offset }
}