use core::sync::atomic::{AtomicU64, Ordering};

use crate::arch::amd64::scheduler::task::TaskIdIndex;

pub struct Notification {
    badges: AtomicU64,
    waiter: Option<TaskIdIndex>,
}

impl Notification {
    pub fn new() -> Self {
        Notification {
            badges: AtomicU64::new(0),
            waiter: None,
        }
    }

    pub fn signal(&mut self, badge: u64) -> Option<TaskIdIndex> {
        self.badges.fetch_or(badge, Ordering::Release);

        self.waiter.take()
    }

    pub fn wait(&mut self, thread_id: TaskIdIndex) -> Option<u64> {
        let badges = self.badges.swap(0, Ordering::Acquire);
        if badges != 0 {
            Some(badges)
        } else {
            self.waiter = Some(thread_id);
            None
        }
    }

    pub fn poll(&self) -> u64 {
        self.badges.load(Ordering::Acquire)
    }

    pub fn clear(&self, mask: u64) {
        self.badges.fetch_and(!mask, Ordering::Release);
    }
}

pub mod badges {
    pub const DATA_READY:    u64 = 1 << 0;
    pub const BUFFER_READY:  u64 = 1 << 1;
    pub const IRQ:           u64 = 1 << 2;
    pub const PROC_EXIT:     u64 = 1 << 3;
    pub const TIMER:         u64 = 1 << 4;
    pub const USER_BASE:     u64 = 1 << 8;
}