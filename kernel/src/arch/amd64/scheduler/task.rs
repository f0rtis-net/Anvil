use alloc::{collections::btree_map::BTreeMap, sync::Arc};
use spin::Mutex;
use x86_64::PhysAddr;

use crate::arch::amd64::{gdt::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR}, memory::vmm::create_new_pt4_from_kernel_pt4, scheduler::stack::{DEFAULT_KERNEL_STACK_SIZE, KernelStack, allocate_kernel_stack}};

pub static TASKS: Mutex<Tasks> = Mutex::new(Tasks::new());

pub type TaskId = usize;

pub(crate) type EntryPoint = extern "C" fn(*const ()) -> ();

const RFLAGS_WITH_IR: u64 = 0x202;

pub struct Tasks {
    next_id: TaskId,
    tasks: BTreeMap<TaskId, Arc<Task>>
}

impl Tasks {
    const fn new() -> Self {
        Self {
            next_id: 0,
            tasks: BTreeMap::new()
        }
    }

    pub fn new_task(&mut self, entrypoint: EntryPoint) -> TaskId {
        let curr_id = self.next_id;
        self.next_id += 1;

        let task = Task::new(curr_id, entrypoint);
        self.tasks.insert(curr_id, Arc::new(task));

        curr_id
    }

    pub fn get_task(&self, id: TaskId) -> Option<Arc<Task>> {
        self.tasks.get(&id).cloned()
    }
}

pub struct Task {
    pub id: TaskId,

    pub registers: TaskRegisters,
    pub page_table: PhysAddr,
    pub kernel_stack: KernelStack
}

impl Task {
    pub fn new(id: TaskId, entrypoint: EntryPoint) -> Self {
        let kernel_stack = allocate_kernel_stack(DEFAULT_KERNEL_STACK_SIZE);

        let registers = TaskRegisters {
            rflags: RFLAGS_WITH_IR,
            ss: KERNEL_DATA_SELECTOR.0 as u64,
            cs: KERNEL_CODE_SELECTOR.0 as u64,
            rsp: kernel_stack.top.as_u64(),
            rip: (entrypoint as u64),
            ..Default::default()
        };

        let region = create_new_pt4_from_kernel_pt4();
        
        Self {
            id,
            registers,
            page_table: region,
            kernel_stack
        }
    }
}

#[derive(Debug, Default)]
#[repr(packed)]
#[allow(dead_code)]
pub struct TaskRegisters {
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