use core::{cell::UnsafeCell, ptr::NonNull, sync::atomic::{AtomicU8, Ordering}};

use x86_64::PhysAddr;

use crate::arch::amd64::{cpu::frames::InterruptFrame, scheduler::{addr_space::AddrSpace, stack::KernelStack}};

pub type TaskIdIndex = u32;
pub type TaskGen = u32;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct TaskId {
    index: TaskIdIndex,
    generation: TaskGen
}

impl TaskId {
    pub fn new(index: TaskIdIndex) -> Self {
        Self { index, generation: 0 }
    }

    pub fn new_full(index: TaskIdIndex, generation: TaskGen) -> Self {
        Self { index, generation }
    }

    pub fn id(&self) -> TaskIdIndex {
        self.index
    }

    pub fn generation(&self) -> TaskGen {
        self.generation
    }
}

pub enum TaskState {
    Running = 0,
    Ready = 1,
    Exiting = 2,
    Sleep = 3,
}

pub struct Task {
    pub id: TaskId,
    pub kernel_stack: KernelStack,
    pub registers: UnsafeCell<TaskRegisters>,
    pub page_table: PhysAddr,
    pub addr_space: Option<AddrSpace>,
    pub task_state: AtomicU8,
}

unsafe impl Sync for Task {}

impl Task {
    pub fn set_state(&self, state: TaskState) {
        self.task_state.store(state as u8, Ordering::Release);
    }

    pub fn get_state(&self) -> TaskState {
        match self.task_state.load(Ordering::Acquire) {
            0 => TaskState::Ready,
            1 => TaskState::Running,
            2 => TaskState::Sleep,
            _ => panic!("invalid task state"),
        }
    }
}

#[derive(Debug, Default)]
#[repr(packed)]
#[allow(dead_code)]
pub(super) struct TaskRegisters {
    pub(super) r15: u64,
    pub(super) r14: u64,
    pub(super) r13: u64,
    pub(super) r12: u64,
    pub(super) rbp: u64,
    pub(super) rbx: u64,

    pub(super) r11: u64,
    pub(super) r10: u64,
    pub(super) r9: u64,
    pub(super) r8: u64,
    pub(super) rax: u64,
    pub(super) rcx: u64,
    pub(super) rdx: u64,
    pub(super) rsi: u64,
    pub(super) rdi: u64,

    pub(super) syscall_number_or_irq_or_error_code: u64,

    pub(super) rip: u64,
    pub(super) cs: u64,
    pub(super) rflags: u64,
    pub(super) rsp: u64,
    pub(super) ss: u64,
}


