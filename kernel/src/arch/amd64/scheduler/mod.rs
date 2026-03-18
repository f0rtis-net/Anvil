use core::{arch::naked_asm, cell::UnsafeCell, ptr::addr_of, sync::atomic::{AtomicU64, Ordering}};
use alloc::{sync::Arc, vec::Vec};
use spin::Once;
use x86_64::{PhysAddr, VirtAddr, instructions::hlt};

pub mod task;
mod stack;
mod cpu_local;
mod elf;
pub mod exec_loader;
pub mod addr_space;
pub mod task_storage;
mod syscall;

use crate::{
    arch::amd64::{
        apic::{PercpuLapic, start_timer}, gdt::set_tss_rsp0, scheduler::{cpu_local::ExecCpu, exec_loader::{make_kernel_task, make_user_task}, syscall::{init_syscall_subsystem, set_per_cpu_TOP_OF_KERNEL_STACK}, task::{Task, TaskId, TaskIdIndex, TaskState}, task_storage::{add_task_to_execute, for_each_task, get_task_by_index, initialize_task_storage, inject_sleeping_task, steal_from_global, table}}
    }, define_per_cpu_struct, irq
};

//pub static PROGRAMM: &[u8] = include_bytes!("../../../../external/user.elf");

pub static CLIENT: &[u8] = include_bytes!("../../../../external/client.elf");

pub static SERVER: &[u8] = include_bytes!("../../../../external/server.elf");

static CPU_NUM: AtomicU64 = AtomicU64::new(0);
static TICK_COUNT: AtomicU64 = AtomicU64::new(0);

struct CpuDescriptorStorage {
    cpus: Vec<UnsafeCell<ExecCpu>>,
}

unsafe impl Sync for CpuDescriptorStorage {}

impl CpuDescriptorStorage {
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

            //TODO: fix this, we need to dynamicaly calc the treshold, when we can steal tasks from curr cpu
            if cpu.tasks.len() < buf.len() {
                continue;
            }

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

static CPU_DESCRIPTORS: Once<CpuDescriptorStorage> = Once::new();

define_per_cpu_struct!{
    pub(super) struct PerCpuSchedulerData {
        cpu_id: usize,
        pub curr_task_id: TaskId,
        in_rescheduling: bool,
        descriptors: &'static CpuDescriptorStorage,
    }
}

pub fn init_scheduler_percpu() -> !{
    let cpu_id = CPU_NUM.fetch_add(1, Ordering::Relaxed) as usize;

    let descriptors = CPU_DESCRIPTORS.get().expect("CPU_DESCRIPTORS not initialized");

    PerCpuSchedulerData::with_guard(|data| {
        data.cpu_id = cpu_id;
        data.curr_task_id = descriptors.cpu(cpu_id).idle_task.id;
        data.in_rescheduling = false;
        data.descriptors = CPU_DESCRIPTORS.get().unwrap();
    });

    init_syscall_subsystem();

    start_timer(&PercpuLapic::get().lapic);

    let my_desc = descriptors.cpu(cpu_id);
    let dummy_rsp: u64 = 0;
    let idle_rsp = unsafe { (*my_desc.idle_task.registers.get()).rsp };
    let idle_cr3 = my_desc.idle_task.addr_space.lock().get_page_table_phys();

    unsafe {
        switch_to_task(
            addr_of!(dummy_rsp),
            idle_rsp,
            idle_cr3.as_u64(),
        );
    }

    unreachable!();
}

pub fn global_init_scheduler(n_cpus: usize) {
    CPU_DESCRIPTORS.call_once(|| CpuDescriptorStorage::new(n_cpus));

    let server = make_user_task(SERVER, 1).unwrap();
    add_task_to_execute(Arc::new(server));

    let client = make_user_task(CLIENT, 2).unwrap();
    add_task_to_execute(Arc::new(client));
}  

pub fn block_current_on_ipc() {
    let my_id = PerCpuSchedulerData::get().cpu_id;
    let my_desc = PerCpuSchedulerData::get().descriptors.cpu_mut(my_id);
    let curr_ptr = my_desc.get_curr_task();

    if curr_ptr.is_null() {
        panic!("block_current_on_ipc: no current task");
    }

    unsafe {
        (*curr_ptr).set_state(TaskState::Sleep);
        let task_rsp_ptr = addr_of!((*(*curr_ptr).registers.get()).rsp);

        my_desc.set_curr_task(core::ptr::null_mut());
        PerCpuSchedulerData::get_mut().curr_task_id = my_desc.idle_task.id;

        let idle_rsp = (*my_desc.idle_task.registers.get()).rsp;
        let idle_cr3 = my_desc.idle_task.addr_space.lock().get_page_table_phys();

        switch_to_task(task_rsp_ptr, idle_rsp, idle_cr3.as_u64());
    }
}

pub fn sleep(ns: u64) {
    let my_id = PerCpuSchedulerData::get().cpu_id;
    let my_desc = PerCpuSchedulerData::get().descriptors.cpu_mut(my_id);
    let curr_ptr = my_desc.get_curr_task();

    let ticks = (ns + 999_999) / 1_000_000;

    if ticks == 0 {
        return;
    }

    let current_tick = TICK_COUNT.load(Ordering::Relaxed);
    let wake_at = current_tick + ticks;

    unsafe { 
        (*curr_ptr).wake_at_tick.lock().store(wake_at, Ordering::Relaxed); 
        (*curr_ptr).set_state(TaskState::Sleep);

        let task_rsp_ptr = addr_of!((*(*curr_ptr).registers.get()).rsp);
        my_desc.set_curr_task(core::ptr::null_mut());
        PerCpuSchedulerData::get_mut().curr_task_id = my_desc.idle_task.id;

        let idle_rsp = (*my_desc.idle_task.registers.get()).rsp;
        let idle_cr3 = my_desc.idle_task.addr_space.lock().get_page_table_phys();

        switch_to_task(task_rsp_ptr, idle_rsp, idle_cr3.as_u64());
    }
}

fn wake_sleeping_tasks() {
    let my_id = PerCpuSchedulerData::get().cpu_id;

    //todo, refactor to bsp, when exists
    if my_id != 1 {
        return;
    }

    let now = TICK_COUNT.load(Ordering::Relaxed);

    let to_wake: Vec<TaskIdIndex> = {
        let tasks = table().tasks.lock();
        tasks.values()
            .filter(|t| {
                if !matches!(t.get_state(), TaskState::Sleep) {
                    return false;
                }
                let wake_at = t.wake_at_tick.lock().load(Ordering::Acquire);
                wake_at != 0 && now >= wake_at
            })
            .map(|t| t.id.id())
            .collect()
    }; 

    for idx in to_wake {
        if let Some(task) = get_task_by_index(idx) {
            task.wake_at_tick.lock().store(0, Ordering::Release);
            task.set_state(TaskState::Ready);
            awaken_task(task);
        }
    }
}

pub fn awaken_task(task: Arc<Task>) {
    task.set_state(TaskState::Ready);
    add_task_to_execute(task);
}

extern "C" fn idle_task() -> ! {
    loop {
        PerCpuSchedulerData::with_guard(|data| {
            data.in_rescheduling = true;
        });

        const STEAL_BATCH: usize = 4;
        let mut global_buf: [Option<Arc<Task>>; STEAL_BATCH] = [None, None, None, None];
        let mut steal_buf:  [Option<Arc<Task>>; STEAL_BATCH] = [None, None, None, None];

        let my_descr = PerCpuSchedulerData::get_mut().descriptors;
        let my_id: usize = PerCpuSchedulerData::get().cpu_id;

        let mut n = my_descr.try_to_steal_into(my_id, &mut steal_buf);

        let my_cpu_data = my_descr.cpu_mut(my_id);

        if n > 0 {
            for slot in steal_buf[..n].iter_mut() {
                if let Some(task) = slot.take() {
                    my_cpu_data.tasks.push(task);
                }
            }
        } else {
            n = steal_from_global(&mut global_buf);
            for slot in global_buf[..n].iter_mut() {
                if let Some(task) = slot.take() {
                    my_cpu_data.tasks.push(task);
                }
            }
        }

        PerCpuSchedulerData::with_guard(|data| {
            data.in_rescheduling = false;
        });

        hlt();
    }
}

fn process_tick() {
    if PerCpuSchedulerData::get().in_rescheduling {
        return;
    }

    let my_id = PerCpuSchedulerData::get().cpu_id;
    let my_desc = PerCpuSchedulerData::get().descriptors.cpu_mut(my_id);
    let curr_ptr = my_desc.get_curr_task();
    let next_task = my_desc.tasks.pop();

    match (curr_ptr.is_null(), next_task) {
        // no tasks to work, go to idle & try to steal
        (true, None) => {
            return;
        },

        // we have a task, so execute it!
        (false, None) => {
            return;
        },

        (true, Some(next)) => {
            let next_ptr = Arc::into_raw(next) as *mut Task;
            unsafe {
                (*next_ptr).set_state(TaskState::Running);
                my_desc.set_curr_task(next_ptr);
                PerCpuSchedulerData::get_mut().curr_task_id = (*next_ptr).id;
                set_per_cpu_TOP_OF_KERNEL_STACK((*next_ptr).kernel_stack.top.as_u64());
                set_tss_rsp0(VirtAddr::new((*next_ptr).kernel_stack.top.as_u64()));
                let idle_rsp_ptr = addr_of!((*my_desc.idle_task.registers.get()).rsp);
                let next_rsp = (*(*next_ptr).registers.get()).rsp;
                let next_cr3 = (*next_ptr).addr_space.lock().get_page_table_phys();

                switch_to_task(idle_rsp_ptr, next_rsp, next_cr3.as_u64());
            }
        },

        (false, Some(next)) => {
            let next_ptr = Arc::into_raw(next) as *mut Task;
            unsafe {
                let task_rsp_ptr = addr_of!((*(*curr_ptr).registers.get()).rsp);
                (*curr_ptr).set_state(TaskState::Ready);
                let curr_arc = Arc::from_raw(curr_ptr);
                my_desc.tasks.push(curr_arc);

                (*next_ptr).set_state(TaskState::Running);
                my_desc.set_curr_task(next_ptr);
                PerCpuSchedulerData::get_mut().curr_task_id = (*next_ptr).id;
                set_per_cpu_TOP_OF_KERNEL_STACK((*next_ptr).kernel_stack.top.as_u64());
                set_tss_rsp0(VirtAddr::new((*next_ptr).kernel_stack.top.as_u64()));

                let next_rsp = (*(*next_ptr).registers.get()).rsp;
                let next_cr3 = (*next_ptr).addr_space.lock().get_page_table_phys();

                switch_to_task(task_rsp_ptr, next_rsp, next_cr3.as_u64());
            }
        }
    }
}

#[unsafe(naked)]
pub(super) unsafe extern "C" fn switch_to_task(
    previous_task_stack_pointer: *const u64,
    next_task_stack_pointer: u64,
    next_page_table: u64,
) {
    naked_asm!(
        "push rax",
        "push rbx",
        "push rcx",
        "push rdx",
        "push rbp",
        "push rsi",
        "push rdi",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push r12",
        "push r13",
        "push r14",
        "push r15",
        "mov [rdi], rsp",
        "mov rsp, rsi",
        "mov cr3, rdx",
        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rdi",
        "pop rsi",
        "pop rbp",
        "pop rdx",
        "pop rcx",
        "pop rbx",
        "pop rax",
        "ret",
    );
}

irq!(0x30, scheduler_tick_irq, |stack| {
    TICK_COUNT.fetch_add(1, Ordering::Relaxed);
    PercpuLapic::get().lapic.eoi();
    wake_sleeping_tasks();
    process_tick();
});
