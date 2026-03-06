use core::{cell::UnsafeCell, hint::spin_loop, sync::atomic::{AtomicI64, AtomicU64, AtomicUsize, Ordering}};

use alloc::{boxed::Box, sync::Arc, vec::Vec};
use spin::Once;

use crate::arch::amd64::scheduler::task::{Task, TaskId};

const TASKS_AMMO: usize = 1024;

const STATE_FREE: u32 = 0;
const STATE_RESERVED: u32 = 1;
const STATE_READY: u32 = 2;

#[repr(align(64))]
struct GlobalTaskSlot {
    gen_state: AtomicU64,
    task: UnsafeCell<Option<Arc<Task>>>,
}

unsafe impl Sync for GlobalTaskSlot {}

pub struct TaskTable {
    slots: Box<[GlobalTaskSlot]>,
    capacity: usize,
}

unsafe impl Sync for TaskTable {}
unsafe impl Send for TaskTable {}

impl TaskTable {
    pub fn new(cap: usize) -> Self {
        let mut v = Vec::with_capacity(cap);
        for _ in 0..cap {
            v.push(GlobalTaskSlot {
                gen_state: AtomicU64::new(0),
                task: UnsafeCell::new(None),
            });
        }
        Self {
            slots: v.into_boxed_slice(),
            capacity: cap,
        }
    }

    #[inline]
    fn pack(generation: u32, state: u32) -> u64 {
        ((generation as u64) << 32) | state as u64
    }

    #[inline]
    fn unpack(v: u64) -> (u32, u32) {
        ((v >> 32) as u32, v as u32)
    }

    pub fn insert(&self, task: Arc<Task>) -> Option<TaskId> {
        let idx = unsafe { (*Arc::as_ptr(&task)).id.id() as usize };
        if idx >= self.capacity {
            return None;
        }

        let slot = &self.slots[idx];
        let old = slot.gen_state.load(Ordering::Acquire);
        let (generation, state) = Self::unpack(old);

        if state != STATE_FREE {
            return None;
        }

        let new_gen = generation.wrapping_add(1);
        let reserved = Self::pack(new_gen, STATE_RESERVED);

        if slot.gen_state.compare_exchange(
            old,
            reserved,
            Ordering::AcqRel,
            Ordering::Relaxed,
        ).is_err() {
            return None;
        }

        unsafe { *slot.task.get() = Some(task); }

        slot.gen_state.store(Self::pack(new_gen, STATE_READY), Ordering::Release);

        Some(TaskId::new_full(idx as u32, new_gen))
    }

    pub fn get_by_index(&self, idx: u32) -> Option<Arc<Task>> {
        let index = idx as usize;
        if index >= self.capacity {
            return None;
        }
        let slot = &self.slots[index];
        let gs = slot.gen_state.load(Ordering::Acquire);
        let (_, state) = Self::unpack(gs);
        if state != STATE_READY {
            return None;
        }
        unsafe { (*slot.task.get()).as_ref().cloned() }
    }

    pub fn get_slot_id(&self, idx: u32) -> Option<TaskId> {
        let index = idx as usize;
        if index >= self.capacity {
            return None;
        }
        let slot = &self.slots[index];
        let gs = slot.gen_state.load(Ordering::Acquire);
        let (generation, state) = Self::unpack(gs);
        if state != STATE_READY {
            return None;
        }
        Some(TaskId::new_full(idx, generation))
    }

    pub fn get(&self, id: TaskId) -> Option<Arc<Task>> {
        let idx = id.id() as usize;
        if idx >= self.capacity {
            return None;
        }

        let slot = &self.slots[idx];
        let gs = slot.gen_state.load(Ordering::Acquire);
        let (generation, state) = Self::unpack(gs);

        if generation != id.generation() || state != STATE_READY {
            return None;
        }

        unsafe { (*slot.task.get()).as_ref().cloned() }
    }

    pub fn remove(&self, id: TaskId) -> bool {
        let idx = id.id() as usize;
        if idx >= self.capacity {
            return false;
        }

        let slot = &self.slots[idx];
        let old = slot.gen_state.load(Ordering::Acquire);
        let (generation, state) = Self::unpack(old);

        if generation != id.generation() || state != STATE_READY {
            return false;
        }

        unsafe { *slot.task.get() = None; }

        let new_gen = generation.wrapping_add(1);
        let free = Self::pack(new_gen, STATE_FREE);

        slot.gen_state
            .compare_exchange(old, free, Ordering::AcqRel, Ordering::Relaxed)
            .is_ok()
    }
}

struct QueueIdSlot {
    sequence: AtomicI64,
    task_id: UnsafeCell<Option<TaskId>>
}

impl QueueIdSlot {
    fn new(seq: i64) -> Self {
        QueueIdSlot {
            sequence: AtomicI64::new(seq * 2), 
            task_id: UnsafeCell::new(None),
        }
    }
}

struct InjectionRing {
    slots:    Box<[QueueIdSlot]>,
    mask:     usize,
    capacity: usize,
    head: AtomicUsize, 
    tail: AtomicUsize, 
}

unsafe impl Sync for InjectionRing {}
unsafe impl Send for InjectionRing {}

impl InjectionRing {
    fn new(capacity: usize) -> Self {
        assert!(capacity.is_power_of_two(), "capacity must be power of two");
        let slots: Vec<QueueIdSlot> = (0..capacity)
            .map(|i| QueueIdSlot::new(i as i64))
            .collect();
        InjectionRing {
            slots: slots.into_boxed_slice(),
            mask: capacity - 1,
            capacity,
            head: AtomicUsize::new(0),
            tail: AtomicUsize::new(0),
        }
    }

    fn try_push(&self, value: TaskId) -> Result<(), TaskId> {
        let mut pos = self.head.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[pos & self.mask];
            let seq  = slot.sequence.load(Ordering::Acquire);
            let diff = seq as isize - (pos * 2) as isize;

            if diff == 0 {
                match self.head.compare_exchange_weak(
                    pos, pos + 1,
                    Ordering::Relaxed, Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        unsafe { *slot.task_id.get() = Some(value); }
                        slot.sequence.store((pos * 2 + 1) as i64, Ordering::Release);
                        return Ok(());
                    }
                    Err(actual) => {
                        pos = actual;
                        continue;
                    }
                }
            } else if diff < 0 {
                return Err(value);
            } else {
                pos = self.head.load(Ordering::Relaxed);
            }
        }
    }


    fn push_spin(&self, mut value: TaskId) {
        loop {
            match self.try_push(value) {
                Ok(()) => return,
                Err(v) => {
                    value = v;
                    spin_loop();
                }
            }
        }
    }

    fn try_pop(&self) -> Option<TaskId> {
        let mut pos = self.tail.load(Ordering::Relaxed);
        loop {
            let slot = &self.slots[pos & self.mask];
            let seq  = slot.sequence.load(Ordering::Acquire);
            let diff = seq as isize - (pos * 2 + 1) as isize;

            if diff == 0 {
                match self.tail.compare_exchange_weak(
                    pos, pos + 1,
                    Ordering::Relaxed, Ordering::Relaxed,
                ) {
                    Ok(_) => {
                        let value = unsafe { (*slot.task_id.get()).take() };
                        slot.sequence.store(
                            ((pos + self.capacity) * 2) as i64,
                            Ordering::Release,
                        );
                        return value;
                    }
                    Err(actual) => {
                        pos = actual;
                        continue;
                    }
                }
            } else if diff < 0 {
                return None;
            } else {
                pos = self.tail.load(Ordering::Relaxed);
            }
        }
    }

    fn pop_batch(&self, buf: &mut [Option<TaskId>], max: usize) -> usize {
        let limit = max.min(buf.len());
        let mut count = 0;
        while count < limit {
            match self.try_pop() {
                Some(v) => { buf[count] = Some(v); count += 1; }
                None    => break,
            }
        }
        count
    }

    fn len_approx(&self) -> usize {
        let h = self.head.load(Ordering::Relaxed);
        let t = self.tail.load(Ordering::Relaxed);
        h.saturating_sub(t)
    }

    fn is_empty(&self) -> bool {
        self.len_approx() == 0
    }
}

static GLOBAL_TASK_TABLE: Once<TaskTable> = Once::new();
static INJECTION_QUEUE: Once<InjectionRing> = Once::new();

#[inline]
fn table() -> &'static TaskTable {
    GLOBAL_TASK_TABLE.get().expect("task table not initialized")
}

#[inline]
fn queue() -> &'static InjectionRing {
    INJECTION_QUEUE.get().expect("injection queue not initialized")
}

pub fn initialize_task_storage() {
    GLOBAL_TASK_TABLE.call_once(|| {
        TaskTable::new(TASKS_AMMO)
    });

    INJECTION_QUEUE.call_once(|| {
        InjectionRing::new(TASKS_AMMO)
    });
}

pub fn add_task_to_execute(task: Task) -> Option<TaskId> {
    let arc_task = Arc::new(task);

    let id = table().insert(arc_task.clone())?;

    queue().push_spin(id);

    Some(id)
}

pub fn get_task(id: TaskId) -> Option<Arc<Task>> {
    table().get(id)
}

pub fn remove_task(id: TaskId) -> bool {
    if table().remove(id) {
        true
    } else {
        false
    }
}

pub fn steal_injected(buf: &mut [Option<TaskId>]) -> usize {
    queue().pop_batch(buf, buf.len())
}

pub fn injection_empty() -> bool {
    queue().is_empty()
}

pub fn get_task_by_index(idx: u32) -> Option<Arc<Task>> {
    table().get_by_index(idx)
}

pub fn inject_sleeping_task(idx: u32) {
    if let Some(full_id) = table().get_slot_id(idx) {
        queue().push_spin(full_id);
    }
}