use x86_64::VirtAddr;

use crate::{arch::amd64::{ipc::message::{OBJ_TYPE_VSPACE, Rights}, memory::pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order}, scheduler::{PerCpuSchedulerData, addr_space::{MapFlags, VmaBacking}, syscall::cap_check::{ExpectedOwner, resolve_cap}, task_storage::get_task_by_index}}};

pub enum MemorySyscallNumbers {
    FrameAlloc  = 0x2,
    VmaMap      = 0x3,
    VmaUnmap    = 0x4,
    Mprotect    = 0x5
}

pub (crate) fn frame_alloc() -> u64 {
    alloc_pages_by_order(0, PAllocFlags::ZEROED | PAllocFlags::KERNEL).expect("frame_alloc: OOM").as_u64()
}

pub(crate) fn vma_map(vspace_cap_idx: u64, vaddr: u64, size: u64, flags: u32) -> u64 {
    let curr_task_id = PerCpuSchedulerData::get().curr_task_id.id();
    let task = get_task_by_index(curr_task_id).unwrap();

    if let Err(e) = resolve_cap(&task, vspace_cap_idx, OBJ_TYPE_VSPACE, ExpectedOwner::CurrentTask, Rights::WRITE) {
        return e.as_syscall_err();
    }

    let map_flags = MapFlags::from_bits_truncate(flags);
    task.addr_space.lock()
        .map(VirtAddr::new(vaddr), size as usize, VmaBacking::Reserved, map_flags)
        .unwrap();

    0
}

pub (crate) fn vma_unmap(vspace_cap_idx: u64, vaddr: u64) -> u64 {
    let curr_task = PerCpuSchedulerData::get().curr_task_id.id();

    let task = get_task_by_index(curr_task).unwrap();

    if let Err(e) = resolve_cap(&task, vspace_cap_idx, OBJ_TYPE_VSPACE, ExpectedOwner::CurrentTask, Rights::WRITE) {
        return e.as_syscall_err();
    }

    task.addr_space.lock().unmap(VirtAddr::new(vaddr)).unwrap();

    0
}

pub (crate) fn mprotect(vspace_cap_idx: u64, vaddr: u64, flags: u32) -> u64 {
    let curr_task = PerCpuSchedulerData::get().curr_task_id.id();

    let task = get_task_by_index(curr_task).unwrap();

    if let Err(e) = resolve_cap(&task, vspace_cap_idx, OBJ_TYPE_VSPACE, ExpectedOwner::CurrentTask, Rights::WRITE) {
        return e.as_syscall_err();
    }

    let map_flags = MapFlags::from_bits_truncate(flags);

    task.addr_space.lock().protect(VirtAddr::new(vaddr), map_flags).unwrap();

    0
}

