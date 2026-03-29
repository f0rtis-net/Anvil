use x86_64::VirtAddr;

use crate::arch::amd64::{ipc::{message::Rights, object_table::{KernelObjType, ObjData, with_object}}, memory::pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order}, scheduler::{PerCpuSchedulerData, addr_space::{MapFlags, VmaBacking}, syscall::cap_check::{CapError, resolve_cap}, task_storage::get_task_by_index}};

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
    let curr = get_task_by_index(curr_task_id).unwrap();

     let (handle, _rights) = match resolve_cap(&curr, vspace_cap_idx, KernelObjType::VSpace, Rights::WRITE) {
        Ok(h) => h,
        Err(e) => return e.as_syscall_err(),
    };

    let target_task_id = match with_object(handle, |obj| {
        match &obj.data {
            ObjData::VSpace(task_id) => Some(*task_id),
            _ => None,
        }
    }).flatten() {
        Some(id) => id,
        None => return CapError::WrongType.as_syscall_err(),
    };

    let target = get_task_by_index(target_task_id).unwrap();
    let map_flags = MapFlags::from_bits_truncate(flags);
    target.tcb.addr_space.lock()
        .map(VirtAddr::new(vaddr), size as usize, VmaBacking::Reserved, map_flags)
        .unwrap();

    0
}

pub(crate) fn vma_unmap(vspace_cap_idx: u64, vaddr: u64) -> u64 {
    let curr_task_id = PerCpuSchedulerData::get().curr_task_id.id();
    let curr = get_task_by_index(curr_task_id).unwrap();

    let (handle, _) = match resolve_cap(&curr, vspace_cap_idx, KernelObjType::VSpace, Rights::WRITE) {
        Ok(h) => h,
        Err(e) => return e.as_syscall_err(),
    };

    let target_task_id = match with_object(handle, |obj| {
        match &obj.data {
            ObjData::VSpace(task_id) => Some(*task_id),
            _ => None,
        }
    }).flatten() {
        Some(id) => id,
        None => return CapError::WrongType.as_syscall_err(),
    };

    let target = get_task_by_index(target_task_id).unwrap();
    target.tcb.addr_space.lock().unmap(VirtAddr::new(vaddr)).unwrap();
    0
}

pub(crate) fn mprotect(vspace_cap_idx: u64, vaddr: u64, flags: u32) -> u64 {
    let curr_task_id = PerCpuSchedulerData::get().curr_task_id.id();
    let curr = get_task_by_index(curr_task_id).unwrap();

    let (handle, _) = match resolve_cap(&curr, vspace_cap_idx, KernelObjType::VSpace, Rights::WRITE) {
        Ok(h) => h,
        Err(e) => return e.as_syscall_err(),
    };

    let target_task_id = match with_object(handle, |obj| {
        match &obj.data {
            ObjData::VSpace(task_id) => Some(*task_id),
            _ => None,
        }
    }).flatten() {
        Some(id) => id,
        None => return CapError::WrongType.as_syscall_err(),
    };

    let target = get_task_by_index(target_task_id).unwrap();
    let map_flags = MapFlags::from_bits_truncate(flags);
    target.tcb.addr_space.lock().protect(VirtAddr::new(vaddr), map_flags).unwrap();
    0
}

