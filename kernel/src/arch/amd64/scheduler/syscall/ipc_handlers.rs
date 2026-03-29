use crate::arch::amd64::{
    ipc::{
        IPC_MANAGER, IpcError, IpcResult, cnode::CapIdx, endpoint::EndpointId, message::{Capability, FastMessage, MsgLabel, Rights}, object_table::{KernelObjType, KernelObject, ObjData, obj_insert, with_object}
    },
    scheduler::{
        awaken_task, block_current_on_ipc,
        syscall::{IpcSyscallArguments, cap_check::resolve_cap},
        task::{Task, TaskRegisters},
        task_storage::get_task_by_index,
    },
};

pub(crate) enum IpcSyscallNumbers {
    IpcSend      = 0x60,
    IpcRecv      = 0x61,
    IpcCall      = 0x62,
    IpcReply     = 0x63,
    IpcEpCreate  = 0x64,
    IpcEpDestroy = 0x65,
}

pub(crate) enum IpcSyscallRetCodes {
    IpcOk              = 0,
    IpcNotReady        = 17,
    IpcInvalidEp       = 10,
    IpcInvalidCap      = 11,
    IpcPermissionDenied = 12,
    IpcUnknown         = 32,
}

fn resolve_endpoint_cap(
    task: &Task,
    cap_idx: CapIdx,
    required_rights: Rights,
) -> Result<EndpointId, IpcSyscallRetCodes> {
    let (handle, _) = resolve_cap(task, cap_idx as u64, KernelObjType::Endpoint, required_rights)
        .map_err(|_| IpcSyscallRetCodes::IpcInvalidCap)?;

    with_object(handle, |obj| {
        match &obj.data {
            ObjData::Endpoint(ep_id) => Some(EndpointId::new(*ep_id as u64)),
            _ => None,
        }
    })
    .flatten()
    .ok_or(IpcSyscallRetCodes::IpcInvalidCap)
}

pub(crate) fn handle_ipc_ep_create(curr_task_id: u32) -> u64 {
    let ep_id = IPC_MANAGER
        .lock()
        .create_endpoint(curr_task_id)
        .unwrap()
        .0;

    let handle = obj_insert(KernelObject::new(
        KernelObjType::Endpoint,
        ObjData::Endpoint(ep_id as u32),
    )).expect("object table full");

    let cap = Capability::new(handle, Rights::ALL);

    let task = get_task_by_index(curr_task_id)
        .expect("handle_ipc_ep_create: task not found");
    let cap_idx = task.tcb.cnode.lock().alloc(cap)
        .expect("handle_ipc_ep_create: CNode full");

    cap_idx as u64
}

pub(crate) fn handle_ipc_ep_destroy(
    curr_task_id: u32,
    cap_idx: u64,
) -> IpcSyscallRetCodes {
    let task = match get_task_by_index(curr_task_id) {
        Some(t) => t,
        None => return IpcSyscallRetCodes::IpcInvalidCap,
    };

    let ep_id = match resolve_endpoint_cap(&task, cap_idx as CapIdx, Rights::ALL) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let handle = {
        let cnode = task.tcb.cnode.lock();
        let cap = cnode.get(cap_idx as CapIdx).unwrap();
        cap.handle
    };

    IPC_MANAGER.lock().destroy_endpoint(ep_id);
    task.tcb.cnode.lock().delete(cap_idx as CapIdx);

    with_object(handle, |obj| obj.dec_ref());

    IpcSyscallRetCodes::IpcOk
}

pub(crate) fn handle_ipc_send(
    curr_task_id: u32,
    ipc: &IpcSyscallArguments,
) -> IpcSyscallRetCodes {
    let task = match get_task_by_index(curr_task_id) {
        Some(t) => t,
        None => return IpcSyscallRetCodes::IpcInvalidCap,
    };

    let ep_id = match resolve_endpoint_cap(&task, ipc.ep_id as CapIdx, Rights::WRITE) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let msg = FastMessage::with_data(MsgLabel::NOTIFY, ipc.msg);
    let result = IPC_MANAGER.lock().handle_send(curr_task_id, ep_id, msg);

    match result {
        IpcResult::WakeReceiver { receiver } => {
            if let Some(task) = get_task_by_index(receiver) {
                awaken_task(task);
            }
            IpcSyscallRetCodes::IpcOk
        }
        IpcResult::NotReady => IpcSyscallRetCodes::IpcNotReady,
        IpcResult::Error(IpcError::InvalidEndpoint) => IpcSyscallRetCodes::IpcInvalidEp,
        IpcResult::Error(_) => IpcSyscallRetCodes::IpcUnknown,
        _ => IpcSyscallRetCodes::IpcOk,
    }
}

pub(crate) fn handle_ipc_recv(
    curr_task_id: u32,
    cap_idx_raw: u64,
    curr_task_regs: &mut TaskRegisters,
) -> IpcSyscallRetCodes {
    let task = match get_task_by_index(curr_task_id) {
        Some(t) => t,
        None => return IpcSyscallRetCodes::IpcInvalidCap,
    };

    let ep_id = match resolve_endpoint_cap(&task, cap_idx_raw as CapIdx, Rights::READ) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let result = IPC_MANAGER.lock().handle_recv(curr_task_id, ep_id);

    match result {
        IpcResult::BlockCurrent => {
            block_current_on_ipc();
            if let Some(msg) = IPC_MANAGER.lock().take_pending_message(curr_task_id) {
                curr_task_regs.rdi = msg.label.0;
                curr_task_regs.rsi = msg.data[0];
                curr_task_regs.rdx = msg.data[1];
                curr_task_regs.r10 = msg.data[2];
                curr_task_regs.r8  = msg.data[3];
            }
            IpcSyscallRetCodes::IpcOk
        }
        IpcResult::Error(_) => IpcSyscallRetCodes::IpcUnknown,
        _ => IpcSyscallRetCodes::IpcOk,
    }
}

pub(crate) fn handle_ipc_call(
    curr_task_id: u32,
    ipc: &IpcSyscallArguments,
    curr_task_regs: &mut TaskRegisters,
) -> IpcSyscallRetCodes {
    let task = match get_task_by_index(curr_task_id) {
        Some(t) => t,
        None => return IpcSyscallRetCodes::IpcInvalidCap,
    };

    let server_ep = match resolve_endpoint_cap(&task, ipc.ep_id as CapIdx, Rights::WRITE) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let reply_ep = match resolve_endpoint_cap(&task, ipc.msg[0] as CapIdx, Rights::READ) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let msg_data = [ipc.msg[1], ipc.msg[2], ipc.msg[3], 0];
    let msg = FastMessage::with_data(MsgLabel::CALL, msg_data);

    let send_result = IPC_MANAGER.lock().handle_send(curr_task_id, server_ep, msg);
    match send_result {
        IpcResult::WakeReceiver { receiver } => {
            if let Some(task) = get_task_by_index(receiver) {
                awaken_task(task);
            }
        }
        IpcResult::NotReady => return IpcSyscallRetCodes::IpcNotReady,
        IpcResult::Error(IpcError::InvalidEndpoint) => return IpcSyscallRetCodes::IpcInvalidEp,
        IpcResult::Error(_) => return IpcSyscallRetCodes::IpcUnknown,
        _ => {}
    }

    let recv_result = IPC_MANAGER.lock().handle_recv(curr_task_id, reply_ep);
    match recv_result {
        IpcResult::BlockCurrent => {
            block_current_on_ipc();
            if let Some(msg) = IPC_MANAGER.lock().take_pending_message(curr_task_id) {
                curr_task_regs.rdi = msg.label.0;
                curr_task_regs.rsi = msg.data[0];
                curr_task_regs.rdx = msg.data[1];
                curr_task_regs.r10 = msg.data[2];
                curr_task_regs.r8  = msg.data[3];
            }
            IpcSyscallRetCodes::IpcOk
        }
        IpcResult::Error(_) => IpcSyscallRetCodes::IpcUnknown,
        _ => IpcSyscallRetCodes::IpcOk,
    }
}

pub(crate) fn handle_ipc_reply(
    curr_task_id: u32,
    ipc: &IpcSyscallArguments,
) -> IpcSyscallRetCodes {
    let task = match get_task_by_index(curr_task_id) {
        Some(t) => t,
        None => return IpcSyscallRetCodes::IpcInvalidCap,
    };

    let ep_id = match resolve_endpoint_cap(&task, ipc.ep_id as CapIdx, Rights::WRITE) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let msg = FastMessage::with_data(MsgLabel::REPLY_OK, ipc.msg);
    let result = IPC_MANAGER.lock().handle_send(curr_task_id, ep_id, msg);

    match result {
        IpcResult::WakeReceiver { receiver } => {
            if let Some(task) = get_task_by_index(receiver) {
                awaken_task(task);
            }
            IpcSyscallRetCodes::IpcOk
        }
        IpcResult::NotReady => IpcSyscallRetCodes::IpcNotReady,
        IpcResult::Error(IpcError::InvalidEndpoint) => IpcSyscallRetCodes::IpcInvalidEp,
        IpcResult::Error(_) => IpcSyscallRetCodes::IpcUnknown,
        _ => IpcSyscallRetCodes::IpcOk,
    }
}