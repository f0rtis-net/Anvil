use crate::arch::amd64::{
    ipc::{
        IPC_MANAGER, IpcError, IpcResult, cnode::CapIdx, endpoint::EndpointId, message::{Capability, FastMessage, MsgLabel, OBJ_TYPE_ENDPOINT, ObjectId, Rights}
    },
    scheduler::{
        awaken_task, block_current_on_ipc,
        syscall::IpcSyscallArguments,
        task::TaskRegisters,
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
    task_id: u32,
    cap_idx: CapIdx,
    required_rights: Rights,
) -> Result<EndpointId, IpcSyscallRetCodes> {
    let task = get_task_by_index(task_id)
        .ok_or(IpcSyscallRetCodes::IpcInvalidCap)?;
    let cnode = task.cnode.lock();
    let cap = cnode.get(cap_idx)
        .ok_or(IpcSyscallRetCodes::IpcInvalidCap)?;
    if !cap.object.is_endpoint() {
        return Err(IpcSyscallRetCodes::IpcInvalidCap);
    }
    if !cap.rights.contains(required_rights) {
        return Err(IpcSyscallRetCodes::IpcPermissionDenied);
    }
    Ok(EndpointId::new(cap.object.raw_id()))
}

pub(crate) fn handle_ipc_ep_create(curr_task_id: u32) -> u64 {
    let ep_id = IPC_MANAGER
        .lock()
        .create_endpoint(curr_task_id)
        .unwrap()
        .0;

    let cap = Capability::new(
        ObjectId::new(OBJ_TYPE_ENDPOINT, ep_id),
        Rights::ALL,
    );

    let task = get_task_by_index(curr_task_id)
        .expect("handle_ipc_ep_create: task not found");
    let cap_idx = task.cnode.lock().alloc(cap)
        .expect("handle_ipc_ep_create: CNode full");

    cap_idx as u64
}

pub(crate) fn handle_ipc_ep_destroy(
    curr_task_id: u32,
    cap_idx: u64,
) -> IpcSyscallRetCodes {
    let ep_id = match resolve_endpoint_cap(
        curr_task_id,
        cap_idx as CapIdx,
        Rights::ALL,
    ) {
        Ok(id) => id,
        Err(e) => return e,
    };

    IPC_MANAGER.lock().destroy_endpoint(ep_id);

    // удаляем cap из CNode
    let task = get_task_by_index(curr_task_id)
        .expect("handle_ipc_ep_destroy: task not found");
    task.cnode.lock().delete(cap_idx as CapIdx);

    IpcSyscallRetCodes::IpcOk
}

pub(crate) fn handle_ipc_send(
    curr_task_id: u32,
    ipc: &IpcSyscallArguments,
) -> IpcSyscallRetCodes {
    let ep_id = match resolve_endpoint_cap(
        curr_task_id,
        ipc.ep_id as CapIdx,
        Rights::WRITE,
    ) {
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
    let ep_id = match resolve_endpoint_cap(
        curr_task_id,
        cap_idx_raw as CapIdx,
        Rights::READ,
    ) {
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
    let server_ep = match resolve_endpoint_cap(
        curr_task_id,
        ipc.ep_id as CapIdx,
        Rights::WRITE,
    ) {
        Ok(id) => id,
        Err(e) => return e,
    };

    let reply_ep = match resolve_endpoint_cap(
        curr_task_id,
        ipc.msg[0] as CapIdx,
        Rights::READ,
    ) {
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
    let ep_id = match resolve_endpoint_cap(
        curr_task_id,
        ipc.ep_id as CapIdx,
        Rights::WRITE,
    ) {
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