use core::cell::UnsafeCell;

use x86_64::PhysAddr;

use crate::{arch::amd64::{cpu::frames::InterruptFrame, scheduler::{addr_space::AddrSpace, stack::KernelStack}}};

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
    Running,
    Ready,
    Exiting,
    Sleep,
}

pub struct Task {
    pub id: TaskId,
    pub kernel_stack: KernelStack,
    pub registers: UnsafeCell<TaskRegisters>,
    pub page_table: PhysAddr,
    pub addr_space: Option<AddrSpace>,
    pub task_state: TaskState
}

unsafe impl Sync for Task {}

impl Task {
    pub fn regs_mut(&self) -> &mut TaskRegisters {
        unsafe { &mut *self.registers.get() }
    }
}

#[derive(Debug, Default, Clone, Copy)]
#[repr(C)]
#[allow(dead_code)]
pub struct TaskRegisters {
    pub rbp: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

impl TaskRegisters {
    pub fn save_from_interrupt(&mut self, frame: &InterruptFrame) {
        self.rbp = frame.rbp;
        self.rax = frame.rax;
        self.rbx = frame.rbx;
        self.rcx = frame.rcx;
        self.rdx = frame.rdx;
        self.rsi = frame.rsi;
        self.rdi = frame.rdi;
        self.r8  = frame.r8;
        self.r9  = frame.r9;
        self.r10 = frame.r10;
        self.r11 = frame.r11;
        self.r12 = frame.r12;
        self.r13 = frame.r13;
        self.r14 = frame.r14;
        self.r15 = frame.r15;

        self.rip = frame.rip;
        self.cs = frame.cs;
        self.rflags = frame.rflags;
        self.rsp = frame.rsp;
        self.ss = frame.ss;
    }
}