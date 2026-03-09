use core::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use core::cell::UnsafeCell;
use crate::arch::amd64::{
    ipc::IpcError,
    ipc::message::FastMessage,
    scheduler::task::TaskIdIndex,
};

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
#[repr(transparent)]
pub struct EndpointId(pub u64);

static NEXT_EP_ID: AtomicU64 = AtomicU64::new(1);

impl EndpointId {
    pub fn alloc() -> Self { EndpointId(NEXT_EP_ID.fetch_add(1, Ordering::Relaxed)) }
    pub fn new(id: u64) -> Self { EndpointId(id) }
}

const WQ_CAP: usize = 16;

struct WaitQueueInner {
    buf:   [Option<TaskIdIndex>; WQ_CAP],
    head:  usize,
    tail:  usize,
    count: usize,
}

struct WaitQueue {
    lock:  AtomicBool,
    inner: UnsafeCell<WaitQueueInner>,
}

unsafe impl Sync for WaitQueue {}

impl WaitQueue {
    const fn new() -> Self {
        Self {
            lock: AtomicBool::new(false),
            inner: UnsafeCell::new(WaitQueueInner {
                buf:   [None; WQ_CAP],
                head:  0,
                tail:  0,
                count: 0,
            }),
        }
    }

    fn acquire(&self) {
        while self.lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            core::hint::spin_loop();
        }
    }

    fn release(&self) {
        self.lock.store(false, Ordering::Release);
    }

    fn inner(&self) -> &mut WaitQueueInner {
        unsafe { &mut *self.inner.get() }
    }

    fn enqueue(&self, id: TaskIdIndex) -> bool {
        self.acquire();
        let ok =  {
            let q = self.inner();
            if q.count < WQ_CAP {
                q.buf[q.tail] = Some(id);
                q.tail  = (q.tail + 1) % WQ_CAP;
                q.count += 1;
                true
            } else {
                false
            }
        };
        self.release();
        ok
    }

    fn dequeue(&self) -> Option<TaskIdIndex> {
        self.acquire();
        let result = {
            let q = self.inner();
            loop {
                if q.count == 0 { break None; }
                let id = q.buf[q.head].take();
                q.head  = (q.head + 1) % WQ_CAP;
                q.count -= 1;
                if id.is_some() { break id; }
            }
        };
        self.release();
        result
    }

    fn cancel(&self, id: TaskIdIndex) -> bool {
        self.acquire();
        let found = {
            let q = self.inner();
            let mut i = q.head;
            let mut found = false;
            for _ in 0..q.count {
                if q.buf[i] == Some(id) {
                    q.buf[i] = None;
                    found = true;
                    break;
                }
                i = (i + 1) % WQ_CAP;
            }
            found
        };
        self.release();
        found
    }

    fn is_empty(&self) -> bool {
        unsafe { (*self.inner.get()).count == 0 }
    }
}

pub struct Endpoint {
    pub id:     EndpointId,
    recv_queue: WaitQueue,
    closed:     bool,
}

impl Endpoint {
    pub fn new() -> Self {
        Self {
            id:         EndpointId::alloc(),
            recv_queue: WaitQueue::new(),
            closed:     false,
        }
    }

    pub fn new_with_id(id: EndpointId) -> Self {
        Self {
            id:         id,
            recv_queue: WaitQueue::new(),
            closed:     false,
        }
    }

    pub fn close(&mut self)         { self.closed = true; }
    pub fn is_closed(&self) -> bool { self.closed }

    pub fn try_send(&mut self, _msg: FastMessage) -> Result<Option<TaskIdIndex>, IpcError> {
        if self.closed {
            return Err(IpcError::EndpointClosed);
        }
        Ok(self.recv_queue.dequeue())
    }

    pub fn try_recv(&mut self, receiver_id: TaskIdIndex) -> Result<(), IpcError> {
        if self.closed {
            return Err(IpcError::EndpointClosed);
        }
        if self.recv_queue.enqueue(receiver_id) {
            Ok(())
        } else {
            Err(IpcError::NotReady) 
        }
    }

    pub fn cancel_recv(&mut self, id: TaskIdIndex) -> bool {
        self.recv_queue.cancel(id)
    }

    pub fn has_waiting_receiver(&self) -> bool {
        !self.recv_queue.is_empty()
    }
}