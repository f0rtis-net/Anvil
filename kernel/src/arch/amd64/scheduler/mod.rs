use core::{arch::asm, cell::UnsafeCell, ptr::null_mut, sync::atomic::{AtomicU64, Ordering}};
use alloc::{sync::Arc, vec::Vec};
use spin::Once;
use x86_64::{PhysAddr, instructions::hlt, registers::{control::{Cr3, Cr3Flags}}, structures::paging::PhysFrame};

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
        apic::{PercpuLapic, start_timer}, cpu::{frames::InterruptFrame, hlt_loop}, scheduler::{cpu_local::ExecCpu, exec_loader::{make_kernel_task, make_user_task}, syscall_handler::{init_syscall_subsystem, set_per_cpu_TOP_OF_KERNEL_STACK}, task::{Task, TaskId, TaskRegisters, TaskState}, task_storage::{add_task_to_execute, get_task, initialize_task_storage, steal_injected}}
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
        pub relaxed_ticks: usize,
        pub cpu_idx: usize,
        pub is_first_run: bool,
        pub in_rescheduling: bool,
    }
}

define_per_cpu_u32!(pub(crate) CURR_TASK_ID);

pub fn init_scheduler(n_cpus: usize) {
    SCHEDULER.call_once(|| Scheduler::new(n_cpus));

    /*let task = make_kernel_task(TaskId::new(0), first_task as u64);
    add_task_to_execute(task).unwrap();

    let task1 = make_kernel_task(TaskId::new(0), first_task as u64);
    add_task_to_execute(task1).unwrap();

    let task1 = make_kernel_task(TaskId::new(0), first_task as u64);
    add_task_to_execute(task1).unwrap();*/
    
    let client = make_user_task(CLIENT, 2).unwrap();
    add_task_to_execute(client).unwrap();

    let server = make_user_task(SERVER, 1).unwrap();
    add_task_to_execute(server).unwrap();
}  

extern "C" fn first_task() {
    hlt_loop()
}

extern "C" fn idle_task() -> ! {
    const BATCH: usize = 32;

    let mut inj_buf: [Option<TaskId>; BATCH] = [None; BATCH];
    let mut steal_buf: Vec<Option<Arc<Task>>> = (0..BATCH).map(|_| None).collect();

    loop {
        let percpu_data = PerCpuSchedulerData::get_mut();
        let scheduler = SCHEDULER.get().unwrap();
        let cpu = scheduler.cpu(percpu_data.cpu_idx);

        if percpu_data.is_first_run {
            percpu_data.is_first_run = false;
        } else if percpu_data.relaxed_ticks < 1000_000_00 {
            percpu_data.relaxed_ticks += 1;
            continue;
        }

        percpu_data.relaxed_ticks = 0;

        let n = steal_injected(&mut inj_buf);

        if n > 0 {
            for i in 0..n {
                if let Some(task_id) = inj_buf[i].take() {
                    if let Some(task) = get_task(task_id) {
                        cpu.tasks.push(task); 
                    }
                }
            }
            percpu_data.in_rescheduling = false;
            continue;
        }

        let n = scheduler.try_to_steal_into(percpu_data.cpu_idx, &mut steal_buf);

        if n > 0 {
            for i in 0..n {
                if let Some(task) = steal_buf[i].take() {
                    cpu.tasks.push(task); 
                }
            }
            percpu_data.in_rescheduling = false;
            continue;
        }

        percpu_data.in_rescheduling = false;
        hlt();
    }
}

pub fn start_scheduler_percpu()  {
    let cpu_id = CPU_NUM.fetch_add(1, Ordering::Relaxed) as usize;

    PerCpuSchedulerData::with_guard(|data| {
        data.is_first_run = true;
        data.cpu_idx = cpu_id;
    });

    init_syscall_subsystem();
    
    let scheduler = SCHEDULER.get().unwrap();
    PerCpuSchedulerData::get_mut().in_rescheduling = true;
    start_timer(&PercpuLapic::get().lapic); 
    force_task_context(scheduler.cpu(cpu_id).idle_task.page_table, &scheduler.cpu(cpu_id).idle_task.regs_mut());
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
                cpu.set_curr_task(null_mut());
                cpu.tasks.push(Arc::from_raw(curr_task));
                set_per_cpu_CURR_TASK_ID(cpu.idle_task.id.id());
                force_task_context(cpu.idle_task.page_table,cpu.idle_task.regs_mut());
            }
            // Task -> Task
            (false, Some(next)) => {
                (*curr_task).regs_mut().save_from_interrupt(frame);
                (*curr_task).task_state = TaskState::Ready;
                
                let curr_arc = Arc::from_raw(curr_task);
                let curr_arc_clone = Arc::clone(&curr_arc);
                let _ = Arc::into_raw(curr_arc); 
                
                let next_ptr = Arc::into_raw(next) as *mut Task;
                cpu.set_curr_task(next_ptr);
                (*next_ptr).task_state = TaskState::Running;
                
                cpu.tasks.push(curr_arc_clone); 
                set_per_cpu_CURR_TASK_ID((*next_ptr).id.id());
                set_per_cpu_TOP_OF_KERNEL_STACK((*next_ptr).kernel_stack.top.as_u64());
                force_task_context((*next_ptr).page_table, (*next_ptr).regs_mut());
            }
        }
    }
}


#[inline(always)]
pub fn force_task_context(pt_phys_addr: PhysAddr, registers: &TaskRegisters) {
    unsafe { 
        let frame = PhysFrame::from_start_address(pt_phys_addr).unwrap();
        Cr3::write(frame, Cr3Flags::empty()); 
    }

    unsafe {
        asm!("mov rsp, {};\
            pop rbp; pop rax; pop rbx; pop rcx; pop rdx; pop rsi; pop rdi; pop r8; pop r9;\
            pop r10; pop r11; pop r12; pop r13; pop r14; pop r15; iretq;",
            in(reg) registers)
    }
}

irq!(0x30, scheduler_tick_irq, |stack| {
    if !PercpuLapic::get().lapic.is_initialized() {
        return;
    }

    PercpuLapic::get().lapic.eoi();
    scheduler_tick(&stack);
});
