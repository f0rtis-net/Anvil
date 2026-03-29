use core::{cell::UnsafeCell, sync::atomic::AtomicU64};

use atomic_enum::atomic_enum;
use spin::Mutex;
use crate::arch::amd64::{ipc::cnode::CNode, scheduler::{addr_space::AddrSpace, stack::KernelStack}};

pub type TaskIdIndex = u32;

#[derive(PartialEq, Eq, PartialOrd, Ord, Clone, Copy)]
pub struct TaskId {
    index: TaskIdIndex,
}

impl TaskId {
    pub fn new(index: TaskIdIndex) -> Self {
        Self { index }
    }

    pub fn id(&self) -> TaskIdIndex {
        self.index
    }
}

#[atomic_enum]
#[repr(u8)]
pub enum TaskState {
    Running = 0,
    Ready = 1,
    Exiting = 2,
    Sleep = 3,
}

pub struct Task {
    pub id: TaskId,
    pub registers: UnsafeCell<TaskRegisters>,
    pub tcb: Tcb
}

pub struct Tcb {
    pub wake_at_tick: Mutex<AtomicU64>,
    pub addr_space: Mutex<AddrSpace>,
    pub kernel_stack: KernelStack,
    pub cnode: Mutex<CNode>,
    pub task_state: AtomicTaskState,
}

unsafe impl Sync for Task {}

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


