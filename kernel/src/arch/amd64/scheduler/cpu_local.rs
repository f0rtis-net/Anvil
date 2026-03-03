use core::{ptr::null_mut, sync::atomic::{AtomicPtr, AtomicUsize, Ordering, fence}};

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use crate::{arch::amd64::scheduler::task::Task};

pub const RQ_CAP: usize = 1024;

pub struct Runqueue {
    top: AtomicUsize,
    bottom: AtomicUsize,
    buf: [AtomicPtr<Task>; RQ_CAP],
}

impl Runqueue {
    pub const fn new() -> Self {
        const NULL: AtomicPtr<Task> = AtomicPtr::new(null_mut());
        Self {
            top: AtomicUsize::new(0),
            bottom: AtomicUsize::new(0),
            buf: [NULL; RQ_CAP],
        }
    }

    pub fn push(&self, task: Arc<Task>) {
        let ptr = Arc::into_raw(task) as *mut Task;
        let b = self.bottom.load(Ordering::Relaxed);
        let t = self.top.load(Ordering::Acquire);

        if b.wrapping_sub(t) >= RQ_CAP {
            panic!("Runqueue overflow! Bottom: {} Top: {}", b, t);
        }
        
        self.buf[b % RQ_CAP].store(ptr, Ordering::Relaxed);
        fence(Ordering::Release);
        self.bottom.store(b.wrapping_add(1), Ordering::Relaxed);
    }

    pub fn pop(&self) -> Option<Arc<Task>> {
        // Load bottom and top BEFORE any decrement.
        // If the queue is already empty (b == t) we must NOT decrement b,
        // because wrapping_sub(1) when b == 0 produces usize::MAX, making the
        // unsigned `t <= b` check spuriously true and leaving bottom corrupted.
        let b_orig = self.bottom.load(Ordering::Relaxed);
        let t_snap  = self.top.load(Ordering::Acquire);

        if b_orig == t_snap {
            return None; // empty — nothing to do
        }

        let b = b_orig.wrapping_sub(1);
        self.bottom.store(b, Ordering::Relaxed);

        // SeqCst fence so stealers see the new bottom before we read top,
        // preventing a double-claim of the last element.
        fence(Ordering::SeqCst);
        let t = self.top.load(Ordering::Relaxed);

        if t <= b {
            let ptr = self.buf[b % RQ_CAP].load(Ordering::Relaxed);

            if t == b {
                // Last element: race with stealers via CAS.
                let won = self.top
                    .compare_exchange(t, t.wrapping_add(1),
                                      Ordering::SeqCst,
                                      Ordering::Relaxed)
                    .is_ok();
                // Restore bottom regardless — top advanced (or stealer took it).
                self.bottom.store(b.wrapping_add(1), Ordering::Relaxed);
                if !won {
                    return None;
                }
            }

            debug_assert!(!ptr.is_null(), "runqueue: null pointer in occupied slot");
            if ptr.is_null() {
                return None;
            }
            Some(unsafe { Arc::from_raw(ptr) })
        } else {
            // All remaining elements were stolen while we decremented; restore.
            self.bottom.store(b.wrapping_add(1), Ordering::Relaxed);
            None
        }
    }

    pub fn steal(&self) -> Option<Arc<Task>> {
        loop {
            let t = self.top.load(Ordering::Acquire);
            fence(Ordering::SeqCst);
            let b = self.bottom.load(Ordering::Acquire);
            
            if t >= b {
                return None; 
            }
            
            let ptr = self.buf[t % RQ_CAP].load(Ordering::Relaxed);
            
            if self.top.compare_exchange(t, t.wrapping_add(1), 
                                         Ordering::SeqCst, 
                                         Ordering::Relaxed).is_ok() {
                if ptr.is_null() {
                    continue;
                }
                return Some(unsafe { Arc::from_raw(ptr) });
            }
        }
    }

    pub fn steal_n(&self, n: usize) -> Vec<Arc<Task>> {
        let mut stolen = Vec::new();
        for _ in 0..n {
            match self.steal() {
                Some(task) => stolen.push(task),
                None => break,
            }
        }
        stolen
    }
}

pub struct ExecCpu {
    pub tasks: Runqueue,
    pub curr_task: *mut Task,
    pub idle_task: Box<Task>
}

unsafe impl Send for ExecCpu {}
unsafe impl Sync for ExecCpu {}

impl ExecCpu {
    pub fn new(idle_task: Task) -> Self {
        Self {
            tasks: Runqueue::new(),
            curr_task: null_mut(),
            idle_task: Box::new(idle_task)
        }
    }

    pub fn accept_n_tasks(&self, tasks: Vec<Arc<Task>>) {
        for task in tasks {
            self.tasks.push(task);
        }
    }

    pub fn get_curr_task(&self) -> *mut Task {
        return self.curr_task
    }

    pub fn set_curr_task(&mut self, curr_task: *mut Task) {
        self.curr_task = curr_task;
    }
}
