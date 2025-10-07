use core::{arch::asm};

use alloc::{collections::{btree_map::BTreeMap, vec_deque::VecDeque}};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{registers::control::Cr3, structures::paging::{FrameAllocator, Size4KiB}, VirtAddr};
use crate::{port, println, task::{Context, Task, TaskManager, TaskPriority, TaskState}};

const PROG: &[u8] = include_bytes!("./main.elf");

#[inline(always)]
pub fn force_task_context(task: &Task) {
    unsafe {
        let (_, flags) = Cr3::read();
        Cr3::write(task.pt_phys, flags);

        asm!("mov rsp, {};\
            pop rbp; pop rax; pop rbx; pop rcx; pop rdx; pop rsi; pop rdi; pop r8; pop r9;\
            pop r10; pop r11; pop r12; pop r13; pop r14; pop r15; iretq;",
            in(reg) &task.ctx);
    }
}

extern "C" fn task_a() {
    let mut pid: usize;

    unsafe {
        asm!(
            "mov rax, 0x27",
            "int 0x80",
            out("rax") pid
        );
    }

    println!("PID of task a: {}", pid);

    println!("Task processed a.");

    unsafe {
        asm!(
            "mov rax, 0x3c",
            "int 0x80",
        );
    }
}

extern "C" fn hlt_task() {
    unsafe {
        loop {
            asm!(
                "hlt",
            );
        }
    }
}

pub struct Scheduler {
    tasks: Mutex<BTreeMap<usize, Task>>,
    runqueue: Mutex<VecDeque<usize>>,
    cur_pid: Mutex<Option<usize>>,
}

impl Scheduler {
    pub fn new() -> Scheduler {
        Scheduler {
            tasks: Mutex::new(BTreeMap::new()),
            runqueue: Mutex::new(VecDeque::new()),
            cur_pid: Mutex::new(None),
        }
    }

    pub fn get_curr_pid(&self) -> usize {
        self.cur_pid.lock().expect("No current task!")
    }

    pub fn remove_task_from_schedule(&self, pid: usize, exit_code: i64) {
        let mut tasks = self.tasks.lock();
        let mut runq = self.runqueue.lock();

        if let Some(task) = tasks.get_mut(&pid) {
            task.exit_code = exit_code;
            task.state = TaskState::Zombie;
        }

        runq.retain(|&p| p != pid);
    }

    pub fn get_exit_code_and_delete_task(&self, pid: usize) -> i64 {
        let mut tasks = self.tasks.lock();

        if let Some(task) = tasks.get(&pid) {
            let exit_code = task.exit_code;

            tasks.remove(&pid);

            return exit_code;
        }

        panic!("Task with current pid is not exist");
    }

    pub fn remove_current_from_schedule(&self, exit_code: i64) {
        if let Some(pid) = *self.cur_pid.lock() {
            self.remove_task_from_schedule(pid, exit_code);
        }
    }

    pub fn schedule_task(&self, task: Task) {
        let pid = task.pid;
        let mut tasks = self.tasks.lock();
        let mut runq = self.runqueue.lock();
        tasks.insert(pid, task);
        runq.push_back(pid);
    }

    pub fn save_current_context(&self, ctxp: *const Context) {
        if let Some(pid) = *self.cur_pid.lock() {
            let mut tasks = self.tasks.lock();
            if let Some(task) = tasks.get_mut(&pid) {
                unsafe {task.ctx = (*ctxp).clone(); }
            }
        }
    }

    fn trampoline_task(&self, task: &Task) {
        unsafe {
            self.cur_pid.force_unlock();
            self.tasks.force_unlock();
            self.runqueue.force_unlock();

            match task.state {
                TaskState::Running => {
                    force_task_context(task) 
                }
                
                _ => panic!("Handled unsupported task state for trampoline")
            }
        }
    }

    pub fn run_next(&self) {
        let mut runq = self.runqueue.lock();
        if runq.is_empty() {
            panic!("No runnable tasks left!");
        }

        let next_pid = runq.pop_front().unwrap();
        runq.push_back(next_pid);

        *self.cur_pid.lock() = Some(next_pid);

        let mut tasks = self.tasks.lock();
        let task = tasks.get_mut(&next_pid).unwrap();

        if task.ticks_left > 0 {
            task.ticks_left -= 1;
            self.trampoline_task(task);
        } else {
            task.ticks_left = task.quant;
            drop(tasks);
            drop(runq);
            self.run_next();
        }

        panic!("Failed context switch, dropped to PANIC STUB!");
    }
}

lazy_static! {
    pub static ref SCHEDULER: Scheduler = Scheduler::new();
}

pub fn test_init(frame_allocator: &mut impl FrameAllocator<Size4KiB>, phys_offset: VirtAddr) {
    let mut manager = TaskManager::new();

    let hlt = manager.create_kernel_task(
        TaskPriority::Low, 
        hlt_task
    );

    let task1 = manager.create_kernel_task(
        TaskPriority::High, 
        task_a
    );

    let task_user = manager.create_user_task(
        TaskPriority::High, 
        PROG,
        frame_allocator,
        phys_offset
    );

    SCHEDULER.schedule_task(hlt);
    SCHEDULER.schedule_task(task1);
    SCHEDULER.schedule_task(task_user);
}

pub unsafe extern "sysv64" fn context_switch(ctx: *const Context) {
    SCHEDULER.save_current_context(ctx);
    port::end_of_interrupt(32);
    SCHEDULER.run_next();
}
