use core::{arch::asm};

use alloc::{collections::{btree_map::BTreeMap, vec_deque::VecDeque}};
use lazy_static::lazy_static;
use spin::Mutex;
use x86_64::{structures::paging::{FrameAllocator, Mapper, Size4KiB}};
use crate::{gdt::{kcs_sel, kds_sel}, port, println, task::{Context, Task, TaskManager, TaskPriority, TaskState}};


#[inline(always)]
pub fn get_context() -> *const Context {
    let ctxp: *const Context;
    unsafe {
        asm!("push r15; push r14; push r13; push r12; push r11; push r10; push r9;\
        push r8; push rdi; push rsi; push rdx; push rcx; push rbx; push rax; push rbp;\
        mov {}, rsp; sub rsp, 0x400;",
        out(reg) ctxp);
    }
    
    ctxp
}

#[inline(always)]
pub fn restore_context(ctxr: &Context) {
    unsafe {
        asm!("mov rsp, {};\
            pop rbp; pop rax; pop rbx; pop rcx; pop rdx; pop rsi; pop rdi; pop r8; pop r9;\
            pop r10; pop r11; pop r12; pop r13; pop r14; pop r15; iretq;",
            in(reg) ctxr);
    }
}

#[inline(never)]
pub fn jmp_to_usermode(code: u64, stack_end: u64) {
    let cs = kcs_sel();
    let ds = kds_sel();
    unsafe {
        asm!("\
            push rax   // stack segment
            push rsi   // rsp
            push 0x200 // rflags (only interrupt bit set)
            push rdx   // code segment
            push rdi   // ret to virtual addr
            iretq",
            in("rdi") code, in("rsi") stack_end, in("dx") cs.0, in("ax") ds.0);
    }
}

extern "C" fn task_a()  {
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

    loop {

    }
}

extern "C" fn hlt_task() {
    println!("Task processed b. Exiting by syscall (exit)");

    unsafe {
        asm!(
            "mov rax, 0x3c",
            "int 0x80",
        );
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
                task.state = TaskState::Running;
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
                    restore_context(&task.ctx) 
                }
                TaskState::Starting => {      
                    jmp_to_usermode(task.ctx.rip, task.ctx.rsp) 
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

pub fn test_init(mapper: &mut impl Mapper<Size4KiB>, frame_allocator: &mut impl FrameAllocator<Size4KiB>) {
    let mut manager = TaskManager::new();

    let idle_task = manager.create_task(
        TaskPriority::High, 
        hlt_task, 
        0x100000, 
        mapper, 
        frame_allocator
    );

    let task1 = manager.create_task(
        TaskPriority::High, 
        task_a, 
        0x2000000, 
        mapper, 
        frame_allocator
    );

    SCHEDULER.schedule_task(idle_task);

    SCHEDULER.schedule_task(task1);
}

pub unsafe extern "sysv64" fn context_switch(ctx: *const Context) {
    SCHEDULER.save_current_context(ctx);
    port::end_of_interrupt(32);
    SCHEDULER.run_next();
}
