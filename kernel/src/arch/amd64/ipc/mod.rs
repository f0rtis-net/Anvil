use spin::Mutex;
use crate::{arch::amd64::{
    ipc::{
        endpoint::{Endpoint, EndpointId},
        message::{Capability, FastMessage, Rights},
        notification::Notification,
    },
    scheduler::task::TaskIdIndex,
}, early_println};

pub mod endpoint;
pub mod message;
pub mod notification;

pub static IPC_MANAGER: Mutex<IpcManager> = Mutex::new(IpcManager::new());

const MAX_ENDPOINTS:     usize = 256;
const MAX_NOTIFICATIONS: usize = 256;

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(u64)]
pub enum IpcError {
    InvalidEndpoint = 1,
    NoPermission    = 2,
    NotReady        = 3,
    Blocked         = 4,
    NoGrant         = 5,
    TooManyCaps     = 6,
    EndpointClosed  = 7,
    Timeout         = 8,
}

pub struct EndpointTable {
    endpoints:     [Option<Endpoint>;     MAX_ENDPOINTS],
    notifications: [Option<Notification>; MAX_NOTIFICATIONS],
}

impl EndpointTable {
    pub const fn new() -> Self {
        Self {
            endpoints:     [const { None }; MAX_ENDPOINTS],
            notifications: [const { None }; MAX_NOTIFICATIONS],
        }
    }

    pub fn create_endpoint(&mut self, task_id: TaskIdIndex) -> Option<EndpointId> {
        for slot in self.endpoints.iter_mut() {
            if slot.is_none() {
                let ep = Endpoint::new_with_id(EndpointId::new(task_id as u64));
                let id = ep.id;
                *slot = Some(ep);
                return Some(id);
            }
        }
        None
    }

    pub fn get_endpoint(&mut self, id: EndpointId) -> Option<&mut Endpoint> {
        self.endpoints.iter_mut()
            .filter_map(|s| s.as_mut())
            .find(|ep| ep.id == id)
    }

    pub fn destroy_endpoint(&mut self, id: EndpointId) {
        for slot in self.endpoints.iter_mut() {
            if let Some(ep) = slot {
                if ep.id == id {
                    ep.close();
                    *slot = None;
                    return;
                }
            }
        }
    }
}

pub enum IpcResult {
    WakeReceiver { receiver: TaskIdIndex, message: FastMessage },
    BlockCurrent,
    NotReady,
    Done,
    Error(IpcError),
}

pub struct IpcManager {
    pub table: EndpointTable,
}

impl IpcManager {
    pub const fn new() -> Self {
        IpcManager { table: EndpointTable::new() }
    }

    pub fn create_endpoint(&mut self, task_id: TaskIdIndex) -> Option<EndpointId> {
        self.table.create_endpoint(task_id)
    }

    pub fn handle_send(
        &mut self,
        sender_id: TaskIdIndex,
        ep_id:     EndpointId,
        msg:       FastMessage,
    ) -> IpcResult {
        let ep = match self.table.get_endpoint(ep_id) {
            Some(ep) => ep,
            None     => return IpcResult::Error(IpcError::InvalidEndpoint),
        };

        match ep.try_send(msg.clone()) {
            Ok(Some(receiver)) => IpcResult::WakeReceiver { receiver, message: msg },
            Ok(None)           => IpcResult::NotReady,
            Err(e)             => IpcResult::Error(e),
        }
    }

    pub fn handle_recv(
        &mut self,
        receiver_id: TaskIdIndex,
        ep_id:       EndpointId,
    ) -> IpcResult {
        let ep = match self.table.get_endpoint(ep_id) {
            Some(ep) => ep,
            None     => return IpcResult::Error(IpcError::InvalidEndpoint),
        };

        match ep.try_recv(receiver_id) {
            Ok(())  => IpcResult::BlockCurrent,
            Err(e)  => IpcResult::Error(e),
        }
    }

    pub fn handle_call(
        &mut self,
        caller_id: TaskIdIndex,
        ep_id:     EndpointId,
        msg:       FastMessage,
    ) -> IpcResult {
        self.handle_send(caller_id, ep_id, msg)
    }

    pub fn handle_reply(
        &mut self,
        caller_id: TaskIdIndex,
        reply_msg: FastMessage,
    ) -> IpcResult {
        IpcResult::WakeReceiver {
            receiver: caller_id,
            message:  reply_msg,
        }
    }

    pub fn validate_caps(
        &self,
        msg:         &FastMessage,
        sender_caps: &[Capability],
    ) -> Result<(), IpcError> {
        for cap in msg.caps() {
            if cap.is_null() { continue; }
            let owned = sender_caps.iter().find(|c| c.object == cap.object);
            match owned {
                None => return Err(IpcError::NoPermission),
                Some(owned_cap) => {
                    if !owned_cap.rights.contains(Rights::GRANT) {
                        return Err(IpcError::NoGrant);
                    }
                    if !owned_cap.rights.contains(cap.rights) {
                        return Err(IpcError::NoPermission);
                    }
                }
            }
        }
        Ok(())
    }
}