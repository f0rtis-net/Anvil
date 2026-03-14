use crate::arch::amd64::{ipc::{IPC_MANAGER, IpcError, IpcResult, endpoint::EndpointId, message::{FastMessage, MsgLabel}}, scheduler::{awaken_task, block_current_on_ipc, syscall::IpcSyscallArguments, task::TaskRegisters, task_storage::get_task_by_index}};

pub (crate) enum IpcSyscallNumbers {
    IpcSend = 0x60,
    IpcRecv = 0x61,
    IpcCall = 0x62,
    IpcReply = 0x63,
    IpcEpCreate = 0x64,
    IpcEpDestroy = 0x65,
}

pub (crate) fn handle_ipc_ep_create(curr_task_id: u32) -> u64 {
    let ep_id = IPC_MANAGER.lock().create_endpoint(curr_task_id).unwrap().0;
    return ep_id;
}

pub (crate) enum IpcSyscallRetCodes {
    IpcOk = 0,
    IpcNotReady = 17,
    IpcInvalidEp = 10,
    IpcUnknown = 32
}

pub (crate) fn handle_ipc_send(curr_task_id: u32, ipc: &IpcSyscallArguments) -> IpcSyscallRetCodes {
    let msg = FastMessage::with_data(MsgLabel::NOTIFY, ipc.msg);

    let result = {
        let mut mgr = IPC_MANAGER.lock();
        mgr.handle_send(curr_task_id, EndpointId::new(ipc.ep_id), msg)
    };

    match result {
        IpcResult::WakeReceiver { receiver } => {
            if let Some(task) = get_task_by_index(receiver) {
                awaken_task(task);
            }
            return IpcSyscallRetCodes::IpcOk;
        }

        IpcResult::NotReady => {
            return IpcSyscallRetCodes::IpcNotReady;
        }

        IpcResult::Error(err) => match err {
            IpcError::InvalidEndpoint => return IpcSyscallRetCodes::IpcInvalidEp,
            _ => return IpcSyscallRetCodes::IpcUnknown,
        },

        _ => return IpcSyscallRetCodes::IpcOk,
    }
}

pub (crate) fn handle_ipc_recv(curr_task_id: u32, sender: u64, curr_task_regs: &mut TaskRegisters) -> IpcSyscallRetCodes {
    let result = IPC_MANAGER.lock().handle_recv(
            curr_task_id,
        EndpointId::new(sender),
    );

    match result {
        IpcResult::BlockCurrent => {
            block_current_on_ipc();

            let msg = IPC_MANAGER.lock().take_pending_message(curr_task_id);
                    
            if let Some(message) = msg {
                curr_task_regs.rdi = message.label.0;
                curr_task_regs.rsi = message.data[0];
                curr_task_regs.rdx = message.data[1];
                curr_task_regs.r10 = message.data[2];
                curr_task_regs.r8  = message.data[3];
            }
            return IpcSyscallRetCodes::IpcOk;
        }
        IpcResult::Error(_) => return IpcSyscallRetCodes::IpcUnknown,
        _ => return IpcSyscallRetCodes::IpcOk,
    }
}

pub (crate) fn handle_ipc_ep_destroy(curr_task_id: u32) -> IpcSyscallRetCodes {
    IPC_MANAGER.lock().destroy_endpoint(EndpointId::new(curr_task_id as u64));
    IpcSyscallRetCodes::IpcOk
}

pub(crate) fn handle_ipc_call(
    curr_task_id: u32,
    ipc: &IpcSyscallArguments,
    curr_task_regs: &mut TaskRegisters,
) -> IpcSyscallRetCodes {
    let server_ep = ipc.ep_id;
    let reply_ep = ipc.msg[0];  
    let msg_data = [ipc.msg[1], ipc.msg[2], ipc.msg[3], 0];

    let msg = FastMessage::with_data(MsgLabel::CALL, msg_data);

    let send_result = {
        let mut mgr = IPC_MANAGER.lock();
        mgr.handle_send(curr_task_id, EndpointId::new(server_ep), msg)
    };

    match send_result {
        IpcResult::WakeReceiver { receiver } => {
            if let Some(task) = get_task_by_index(receiver) {
                awaken_task(task);
            }
        }
        IpcResult::NotReady => return IpcSyscallRetCodes::IpcNotReady,
        IpcResult::Error(err) => match err {
            IpcError::InvalidEndpoint => return IpcSyscallRetCodes::IpcInvalidEp,
            _ => return IpcSyscallRetCodes::IpcUnknown,
        },
        _ => {}
    }

    let recv_result = IPC_MANAGER.lock().handle_recv(
        curr_task_id,
        EndpointId::new(reply_ep),
    );

    match recv_result {
        IpcResult::BlockCurrent => {
            block_current_on_ipc();

            let msg = IPC_MANAGER.lock().take_pending_message(curr_task_id);
            if let Some(message) = msg {
                curr_task_regs.rdi = message.label.0;
                curr_task_regs.rsi = message.data[0];
                curr_task_regs.rdx = message.data[1];
                curr_task_regs.r10 = message.data[2];
                curr_task_regs.r8  = message.data[3];
            }
            return IpcSyscallRetCodes::IpcOk;
        }
        IpcResult::Error(_) => return IpcSyscallRetCodes::IpcUnknown,
        _ => return IpcSyscallRetCodes::IpcOk,
    }
}

pub(crate) fn handle_ipc_reply(
    curr_task_id: u32,
    ipc: &IpcSyscallArguments,
) -> IpcSyscallRetCodes {
    let msg = FastMessage::with_data(MsgLabel::REPLY_OK, ipc.msg);

    let result = {
        let mut mgr = IPC_MANAGER.lock();
        mgr.handle_send(curr_task_id, EndpointId::new(ipc.ep_id), msg)
    };

    match result {
        IpcResult::WakeReceiver { receiver } => {
            if let Some(task) = get_task_by_index(receiver) {
                awaken_task(task);
            }
            IpcSyscallRetCodes::IpcOk
        }
        IpcResult::NotReady => IpcSyscallRetCodes::IpcNotReady,
        IpcResult::Error(err) => match err {
            IpcError::InvalidEndpoint => IpcSyscallRetCodes::IpcInvalidEp,
            _ => IpcSyscallRetCodes::IpcUnknown,
        },
        _ => IpcSyscallRetCodes::IpcOk,
    }
}