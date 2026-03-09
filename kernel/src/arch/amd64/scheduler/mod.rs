use core::{arch::{naked_asm}, cell::UnsafeCell, ptr::null_mut, sync::atomic::{AtomicU64, Ordering}};
use alloc::{sync::Arc, vec::Vec};
use spin::Once;
use x86_64::{PhysAddr, instructions::{hlt}};

pub mod task;
mod stack;
mod cpu_local;
mod elf;
pub mod exec_loader;
mod addr_space;
pub mod task_storage;
mod syscall_handler;

use crate::{
    arch::amd64::{
        apic::{PercpuLapic, start_timer}, cpu::{frames::InterruptFrame, hlt_loop}, scheduler::{cpu_local::ExecCpu, exec_loader::{make_kernel_task, make_user_task}, syscall_handler::{init_syscall_subsystem, set_per_cpu_TOP_OF_KERNEL_STACK}, task::{Task, TaskId, TaskRegisters, TaskState}, task_storage::{add_task_to_execute, initialize_task_storage, steal_from_global}}
    }, define_per_cpu_struct, define_per_cpu_u32, early_println, irq, isr
};

pub struct Scheduler {
    pub cpus: Vec<UnsafeCell<ExecCpu>>,
}

//pub static PROGRAMM: &[u8] = include_bytes!("../../../../external/user.elf");

pub static CLIENT: &[u8] = include_bytes!("../../../../external/client.elf");

pub static SERVER: &[u8] = include_bytes!("../../../../external/server.elf");

isr!(0x3, int3_handler, |stack| {
    early_println!("Handled int3 from user task");
});

unsafe impl Sync for Scheduler {}

impl Scheduler {
    pub fn new(n_cpus: usize) -> Self {
        let mut cpus = Vec::with_capacity(n_cpus);
        initialize_task_storage();
        for _ in 0..n_cpus { cpus.push(UnsafeCell::new(ExecCpu::new(make_kernel_task(TaskId::new(0), idle_task as u64)))); }
        Self { cpus }
    }

    pub fn cpu(&self, cpu: usize) -> &ExecCpu {
        unsafe { return &*self.cpus[cpu].get(); }
    }

    pub fn cpu_mut(&self, cpu: usize) -> &mut ExecCpu {
        unsafe { return &mut *self.cpus[cpu].get(); }
    }

    pub fn try_to_steal_into(&self, me: usize, buf: &mut [Option<Arc<Task>>]) -> usize {
        let mut count = 0;
        let steal_batch = 2;

        for (idx, cpu_cell) in self.cpus.iter().enumerate() {
            if idx == me { continue; }

            let cpu = unsafe { &*cpu_cell.get() };
            let tasks = cpu.tasks.steal_n(steal_batch); 

            for task in tasks {
                if count >= buf.len() { return count; }
                buf[count] = Some(task); 
                count += 1;
            }
        }

        count
    }
}

static SCHEDULER: Once<Scheduler> = Once::new();
static CPU_NUM: AtomicU64 = AtomicU64::new(0);

define_per_cpu_struct! {
    struct PerCpuSchedulerData {
        pub cpu_idx: usize,
        pub in_rescheduling: bool,
    }
}

define_per_cpu_u32!(pub CURR_TASK_ID);

pub fn init_scheduler(n_cpus: usize) {
    SCHEDULER.call_once(|| Scheduler::new(n_cpus));

    let server = make_user_task(SERVER, 1).unwrap();
    add_task_to_execute(server);

    let client = make_user_task(CLIENT, 2).unwrap();
    add_task_to_execute(client);
}  

extern "C" fn idle_task() -> ! {
    const STEAL_BATCH: usize = 4;
    let mut global_buf: [Option<Arc<Task>>; STEAL_BATCH] = [None, None, None, None];
    let mut steal_buf:  [Option<Arc<Task>>; STEAL_BATCH] = [None, None, None, None];

    loop {
        let percpu_data = PerCpuSchedulerData::get_mut();
        let scheduler   = SCHEDULER.get().unwrap();
        let cpu         = scheduler.cpu_mut(percpu_data.cpu_idx);

        percpu_data.in_rescheduling = true;

        let n = steal_from_global(&mut global_buf);
        for slot in global_buf[..n].iter_mut() {
            if let Some(task) = slot.take() {
                cpu.tasks.push(task);
            }
        }

        let n = scheduler.try_to_steal_into(percpu_data.cpu_idx, &mut steal_buf);
        for slot in steal_buf[..n].iter_mut() {
            if let Some(task) = slot.take() {
                cpu.tasks.push(task);
            }
        }

        if let Some(next) = cpu.tasks.pop() {
            percpu_data.in_rescheduling = false;
            unsafe {
                let next_ptr = Arc::into_raw(next) as *mut Task;
                (*next_ptr).task_state = TaskState::Running;
                cpu.set_curr_task(next_ptr);
                set_per_cpu_CURR_TASK_ID((*next_ptr).id.id());
                set_per_cpu_TOP_OF_KERNEL_STACK((*next_ptr).kernel_stack.top.as_u64());
                force_task_context((*next_ptr).page_table, (*next_ptr).regs_mut());
            }
        }

        percpu_data.in_rescheduling = false;
        hlt();
    }
}

pub fn start_scheduler_percpu()  {
    let cpu_id = CPU_NUM.fetch_add(1, Ordering::Relaxed) as usize;

    PerCpuSchedulerData::with_guard(|data| {
        data.cpu_idx = cpu_id;
    });

    init_syscall_subsystem();
    
    let scheduler = SCHEDULER.get().unwrap();
    start_timer(&PercpuLapic::get().lapic); 
    unsafe { force_task_context(scheduler.cpu(cpu_id).idle_task.page_table, &scheduler.cpu(cpu_id).idle_task.regs_mut()); }
}

fn scheduler_tick(frame: &InterruptFrame) {
    if PerCpuSchedulerData::get().in_rescheduling {
        return;
    }

    let cpu_id = PerCpuSchedulerData::get().cpu_idx;
    let scheduler = SCHEDULER.get().unwrap();
    let cpu = scheduler.cpu_mut(cpu_id);

    let curr_task = cpu.get_curr_task();
    let next_task = cpu.tasks.pop();

    unsafe {
        match (curr_task.is_null(), next_task) {
            // Idle -> Idle
            (true, None) => {
                cpu.idle_task.regs_mut().save_from_interrupt(frame);
                set_per_cpu_CURR_TASK_ID(cpu.idle_task.id.id());
                force_task_context(cpu.idle_task.page_table, cpu.idle_task.regs_mut());
            }
            // Idle -> Task
            (true, Some(next)) => {
                cpu.idle_task.regs_mut().save_from_interrupt(frame);
                let next_ptr = Arc::into_raw(next) as *mut Task;
                (*next_ptr).task_state = TaskState::Running;
                cpu.set_curr_task(next_ptr);
                set_per_cpu_CURR_TASK_ID((*next_ptr).id.id());
                set_per_cpu_TOP_OF_KERNEL_STACK((*next_ptr).kernel_stack.top.as_u64());
                force_task_context((*next_ptr).page_table, (*next_ptr).regs_mut());
            }
            // Task -> Idle
            (false, None) => {
                (*curr_task).regs_mut().save_from_interrupt(frame);
                set_per_cpu_TOP_OF_KERNEL_STACK((*curr_task).kernel_stack.top.as_u64()); 
                cpu.set_curr_task(null_mut());
                cpu.tasks.push(Arc::from_raw(curr_task));
                set_per_cpu_CURR_TASK_ID(cpu.idle_task.id.id());
                force_task_context(cpu.idle_task.page_table, cpu.idle_task.regs_mut());
            }
            // Task -> Task
            (false, Some(next)) => {
                (*curr_task).regs_mut().save_from_interrupt(frame);
                (*curr_task).task_state = TaskState::Ready;
                let curr_arc = Arc::from_raw(curr_task);
                let next_ptr = Arc::into_raw(next) as *mut Task;
                (*next_ptr).task_state = TaskState::Running;
                cpu.set_curr_task(next_ptr);
                cpu.tasks.push(curr_arc);
                set_per_cpu_CURR_TASK_ID((*next_ptr).id.id());
                set_per_cpu_TOP_OF_KERNEL_STACK((*next_ptr).kernel_stack.top.as_u64());
                force_task_context((*next_ptr).page_table, (*next_ptr).regs_mut());
            }
        }
    }
}


#[unsafe(naked)]
pub unsafe extern "C" fn force_task_context(pt_phys_addr: PhysAddr, registers: &TaskRegisters) {
    naked_asm!(
        "mov cr3, rdi",   
        "mov rsp, rsi",   
        "pop rbp",
        "pop rax",
        "pop rbx",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",
        "pop r8",
        "pop r9",
        "pop r10",
        "pop r11",
        "pop r12",
        "pop r13",
        "pop r14",
        "pop r15",
        "iretq",
    )
}

pub(super) fn block_current_ipc(regs: &TaskRegisters) -> ! {
    let cpu_id = PerCpuSchedulerData::get().cpu_idx;
    let cpu    = SCHEDULER.get().unwrap().cpu_mut(cpu_id);

    unsafe {
        let curr_ptr = cpu.get_curr_task();

        *(*curr_ptr).regs_mut() = *regs;
        (*curr_ptr).task_state  = TaskState::Sleep;

        set_per_cpu_CURR_TASK_ID(cpu.idle_task.id.id());

        cpu.set_curr_task(core::ptr::null_mut());

        core::arch::asm!("swapgs", options(nostack, nomem));

        force_task_context(cpu.idle_task.page_table, cpu.idle_task.regs_mut());
    };

    hlt_loop()
}

irq!(0x30, scheduler_tick_irq, |stack| {
    PercpuLapic::get().lapic.eoi();
    scheduler_tick(&stack);
});
