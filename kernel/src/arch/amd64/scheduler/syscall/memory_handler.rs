use x86_64::VirtAddr;

use crate::arch::amd64::{memory::pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order}, scheduler::{PerCpuSchedulerData, addr_space::{MapFlags, VmaBacking}, task_storage::get_task_by_index}};

pub enum MemorySyscallNumbers {
    FrameAlloc  = 0x2,
    VmaMap      = 0x3,
    VmaUnmap    = 0x4,
    Mprotect    = 0x5
}

pub (crate) fn frame_alloc() -> u64 {
    alloc_pages_by_order(0, PAllocFlags::ZEROED | PAllocFlags::KERNEL).expect("frame_alloc: OOM").as_u64()
}

pub (crate) fn vma_map(vaddr: u64, size: u64, flags: u32) -> u64 {
    let curr_task = PerCpuSchedulerData::get().curr_task_id.id();

    let task = get_task_by_index(curr_task).unwrap();
    let map_flags = MapFlags::from_bits_truncate(flags);

    task.addr_space.lock().map(VirtAddr::new(vaddr), size as usize, VmaBacking::Reserved, map_flags).unwrap();

    0
}

pub (crate) fn vma_unmap(vaddr: u64) -> u64 {
    let curr_task = PerCpuSchedulerData::get().curr_task_id.id();

    let task = get_task_by_index(curr_task).unwrap();

    task.addr_space.lock().unmap(VirtAddr::new(vaddr)).unwrap();

    0
}

pub (crate) fn mprotect(vaddr: u64, flags: u32) -> u64 {
    let curr_task = PerCpuSchedulerData::get().curr_task_id.id();

    let task = get_task_by_index(curr_task).unwrap();

    let map_flags = MapFlags::from_bits_truncate(flags);

    task.addr_space.lock().protect(VirtAddr::new(vaddr), map_flags).unwrap();

    0
}

