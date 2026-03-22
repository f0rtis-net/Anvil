use core::{cell::UnsafeCell, ptr::NonNull, sync::atomic::{AtomicU8, AtomicU64, Ordering}};

use spin::Mutex;
use crate::arch::amd64::{cpu::frames::InterruptFrame, ipc::cnode::CNode, scheduler::{addr_space::AddrSpace, stack::KernelStack}};

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
    pub addr_space: Mutex<AddrSpace>,
    pub task_state: AtomicU8,
    pub wake_at_tick: Mutex<AtomicU64>,
    pub cnode: Mutex<CNode>
}

unsafe impl Sync for Task {}

impl Task {
    pub fn set_state(&self, state: TaskState) {
        self.task_state.store(state as u8, Ordering::Release);
    }

    pub fn get_state(&self) -> TaskState {
        match self.task_state.load(Ordering::Acquire) {
            0 => TaskState::Running,
            1 => TaskState::Ready,
            2 => TaskState::Exiting,
            3 => TaskState::Sleep,
            _ => panic!("invalid task state"),
        }
    }
}

#[derive(Debug, Default)]
#[repr(packed)]
#[allow(dead_code)]
pub struct TaskRegisters {
    pub r15: u64,
    pub r14: u64,
    pub r13: u64,
    pub r12: u64,
    pub rbp: u64,
    pub rbx: u64,

    pub r11: u64,
    pub r10: u64,
    pub r9: u64,
    pub r8: u64,
    pub rax: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,

    pub syscall_number_or_irq_or_error_code: u64,

    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}


