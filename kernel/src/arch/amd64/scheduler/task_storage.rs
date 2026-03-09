use alloc::{collections::{VecDeque, btree_map::BTreeMap}, sync::Arc};
use spin::{Mutex, Once};
use crate::arch::amd64::scheduler::task::{Task, TaskId, TaskIdIndex};

pub struct TaskTable {
    tasks: Mutex<BTreeMap<TaskIdIndex, Arc<Task>>>,
}

impl TaskTable {
    pub fn new() -> Self {
        Self { tasks: Mutex::new(BTreeMap::new()) }
    }

    pub fn insert(&self, task: Arc<Task>) {
        self.tasks.lock().insert(task.id.id(), task);
    }

    pub fn get_by_index(&self, idx: TaskIdIndex) -> Option<Arc<Task>> {
        self.tasks.lock().get(&idx).cloned()
    }

    pub fn remove(&self, idx: TaskIdIndex) {
        self.tasks.lock().remove(&idx);
    }
}

pub struct GlobalRunQueue {
    inner: Mutex<VecDeque<Arc<Task>>>,
}

impl GlobalRunQueue {
    pub const fn new() -> Self {
        Self { inner: Mutex::new(VecDeque::new()) }
    }

    pub fn push(&self, task: Arc<Task>) {
        self.inner.lock().push_back(task);
    }

    pub fn pop(&self) -> Option<Arc<Task>> {
        self.inner.lock().pop_front()
    }

    pub fn is_empty(&self) -> bool {
        self.inner.lock().is_empty()
    }
}

static TASK_TABLE:        Once<TaskTable>      = Once::new();
static GLOBAL_RUN_QUEUE:  Once<GlobalRunQueue> = Once::new();

#[inline] fn table() -> &'static TaskTable      { TASK_TABLE.get().expect("task table not initialized") }
#[inline] fn global_queue() -> &'static GlobalRunQueue { GLOBAL_RUN_QUEUE.get().expect("global run queue not initialized") }

pub fn initialize_task_storage() {
    TASK_TABLE.call_once(TaskTable::new);
    GLOBAL_RUN_QUEUE.call_once(|| GlobalRunQueue::new());
}

pub fn add_task_to_execute(task: Task) -> TaskId {
    let id = task.id;
    let arc = Arc::new(task);
    table().insert(arc.clone());
    global_queue().push(arc);
    id
}

pub fn get_task_by_index(idx: TaskIdIndex) -> Option<Arc<Task>> {
    table().get_by_index(idx)
}

pub fn remove_task(idx: TaskIdIndex) {
    table().remove(idx);
}

pub fn inject_sleeping_task(idx: TaskIdIndex) {
    if let Some(task) = table().get_by_index(idx) {
        global_queue().push(task);
    }
}

pub fn steal_from_global(buf: &mut [Option<Arc<Task>>]) -> usize {
    let mut count = 0;
    while count < buf.len() {
        match global_queue().pop() {
            Some(t) => { buf[count] = Some(t); count += 1; }
            None    => break,
        }
    }
    count
}

pub fn global_queue_empty() -> bool {
    global_queue().is_empty()
}