use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::amd64::{ipc::{IpcError, message::FastMessage}, scheduler::task::{TaskId, TaskIdIndex}};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct EndpointId(pub u64);

static NEXT_EP_ID: AtomicU64 = AtomicU64::new(1);

impl EndpointId {
    pub fn alloc() -> Self {
        EndpointId(NEXT_EP_ID.fetch_add(1, Ordering::Relaxed))
    }
}

#[derive(Clone)]
pub enum IpcState {
    Running,
    BlockedOnSend(FastMessage),
    BlockedOnRecv,
    BlockedOnReply,
}

#[derive(Clone)]
pub struct PendingSend {
    pub task_id: TaskIdIndex,
    pub message:   FastMessage,
}

pub struct Endpoint {
    pub id: EndpointId,

    send_queue: [Option<PendingSend>; 16],
    send_head:  usize,
    send_tail:  usize,
    send_count: usize,

    receiver: Option<TaskIdIndex>,

    closed: bool,
}

impl Endpoint {
    pub const QUEUE_SIZE: usize = 16;

    pub fn new() -> Self {
        Endpoint {
            id:         EndpointId::alloc(),
            send_queue: [const { None }; 16],
            send_head:  0,
            send_tail:  0,
            send_count: 0,
            receiver:   None,
            closed:     false,
        }
    }

    pub fn close(&mut self) {
        self.closed = true;
    }

    pub fn is_closed(&self) -> bool {
        self.closed
    }

    pub fn try_send(
        &mut self,
        sender_id: TaskIdIndex,
        msg: FastMessage,
    ) -> Result<Option<TaskIdIndex>, IpcError> {
        if self.closed {
            return Err(IpcError::EndpointClosed);
        }

        if let Some(receiver_id) = self.receiver.take() {
            Ok(Some(receiver_id))
        } else {
            if self.send_count >= Self::QUEUE_SIZE {
                return Err(IpcError::NotReady);
            }
            self.send_queue[self.send_tail] = Some(PendingSend {
                task_id: sender_id,
                message: msg,
            });
            self.send_tail = (self.send_tail + 1) % Self::QUEUE_SIZE;
            self.send_count += 1;
            Ok(None)
        }
    }

    pub fn try_recv(
        &mut self,
        receiver_id: TaskIdIndex,
    ) -> Result<Option<PendingSend>, IpcError> {
        if self.closed {
            return Err(IpcError::EndpointClosed);
        }

        if self.send_count > 0 {
            let pending = self.send_queue[self.send_head].take().unwrap();
            self.send_head = (self.send_head + 1) % Self::QUEUE_SIZE;
            self.send_count -= 1;
            Ok(Some(pending))
        } else {
            self.receiver = Some(receiver_id);
            Ok(None)
        }
    }

    pub fn cancel_recv(&mut self) -> Option<TaskIdIndex> {
        self.receiver.take()
    }

    pub fn has_pending_senders(&self) -> bool {
        self.send_count > 0
    }

    pub fn has_receiver(&self) -> bool {
        self.receiver.is_some()
    }
}