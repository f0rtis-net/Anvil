use crate::arch::amd64::scheduler::{sleep, task::{TaskId, TaskIdIndex}, task_storage::get_task_by_index};

pub enum ThreadSyscallNums {
    ThreadSleep = 0x99,
    ThreadExit = 0x11
}

pub (crate) fn thread_exit(self_id: u64, code: u64) -> ! {
    //let task = get_task_by_index(self_id as TaskIdIndex).expect("Task not found");
    //drop(task.addr_space.lock());
    todo!()
}

pub (crate) fn thread_sleep(ns: u64) -> u64 {
    sleep(ns);

    0
}