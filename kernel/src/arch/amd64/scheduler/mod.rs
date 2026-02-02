use core::{arch::naked_asm};

use alloc::{sync::Arc, vec::Vec};

use crate::{arch::amd64::{apic::{PercpuLapic, start_timer}, scheduler::task::{TASKS, Task, TaskId, TaskRegisters}}, define_per_cpu_struct, define_per_cpu_u32, define_per_cpu_u64, early_println, irq};

mod task;
mod stack;

define_per_cpu_u64!(CPU_TICKED);

define_per_cpu_struct! {
    struct Runqueue {
        pending_tasks: Vec<TaskId>
    }
}

extern "C" fn idle_task(_arg: *const ()) {
    early_println!("Cpu handled idle task!");
    loop {
        early_println!("LapicID: {} | Ticked: {}", PercpuLapic::get().lapic.id(), get_per_cpu_no_guard_CPU_TICKED());
    }
}

impl Runqueue {
    pub fn new() -> Self {
        Self {
            pending_tasks: Vec::new()
        }
    }

    pub fn pop_next_ready(&mut self) -> Option<TaskId> {
        return self.pending_tasks.pop()
    }
}

define_per_cpu_u32!(CPU_CURR_TASK_ID);

pub(crate) fn current_task_id() -> TaskId {
    let id = get_per_cpu_no_guard_CPU_CURR_TASK_ID();
    id as usize
}

fn current_task() -> Arc<Task> {
    TASKS
        .lock()
        .get_task(current_task_id()).unwrap()
}

pub fn init_sheduler_for_percpu() {
    let idle_task_id = TASKS.lock().new_task(idle_task);
    set_per_cpu_CPU_CURR_TASK_ID(idle_task_id as u32);
}

pub fn start_scheduler() -> ! {
    let task = current_task();
    set_per_cpu_CPU_CURR_TASK_ID(task.id as u32);
    start_timer(&PercpuLapic::get().lapic);
    first_trampoline(&task.registers as *const _);
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn first_trampoline(task_regs: *const TaskRegisters) -> ! {
    naked_asm!(
        "mov rsp, rdi",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",

        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",

        "add rsp, 8",

        "iretq",
    );
}

#[unsafe(naked)]
#[unsafe(no_mangle)]
pub extern "C" fn switch_to(
    prev: *mut TaskRegisters,
    next: *const TaskRegisters,
) -> ! {
    naked_asm!(
        //save curr task
        "mov [rdi + 0x00], r15",
        "mov [rdi + 0x08], r14",
        "mov [rdi + 0x10], r13",
        "mov [rdi + 0x18], r12",
        "mov [rdi + 0x20], rbp",
        "mov [rdi + 0x28], rbx",

        "mov [rdi + 0x30], r11",
        "mov [rdi + 0x38], r10",
        "mov [rdi + 0x40], r9",
        "mov [rdi + 0x48], r8",
        "mov [rdi + 0x50], rax",
        "mov [rdi + 0x58], rcx",
        "mov [rdi + 0x60], rdx",
        "mov [rdi + 0x68], rsi",
        "mov [rdi + 0x70], rdi",

        "mov qword ptr [rdi + 0x78], 0",

        "lea rax, [rip + 0f]",
        "mov [rdi + 0x80], rax", // rip
        "mov ax, cs",
        "mov [rdi + 0x88], rax", // cs
        "pushfq",
        "pop qword ptr [rdi + 0x90]", // rflags
        "mov [rdi + 0x98], rsp", // rsp
        "mov ax, ss",
        "mov [rdi + 0xA0], rax", // ss

        "0:",

        //next context
        "mov rsp, rsi",

        "pop r15",
        "pop r14",
        "pop r13",
        "pop r12",
        "pop rbp",
        "pop rbx",

        "pop r11",
        "pop r10",
        "pop r9",
        "pop r8",
        "pop rax",
        "pop rcx",
        "pop rdx",
        "pop rsi",
        "pop rdi",

        "add rsp, 8",

        "iretq",
    );
}

irq!(0x30, scheduler_tick_irq, |stack| {
    inc_per_cpu_CPU_TICKED();
    PercpuLapic::get().lapic.eoi();
});