use core::arch::naked_asm;

use spin::Mutex;
use x86_64::{VirtAddr, registers::{control::{Efer, EferFlags}, model_specific::{LStar, SFMask}, rflags::RFlags}};

use crate::{arch::amd64::{gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR}, ipc::{IPC_MANAGER, IpcError, IpcResult, endpoint::EndpointId, message::{FastMessage, MsgLabel}}, scheduler::{PerCpuSchedulerData, awaken_task, block_current_on_ipc, syscall::ipc_handlers::{IpcSyscallNumbers, handle_ipc_call, handle_ipc_ep_create, handle_ipc_ep_destroy, handle_ipc_recv, handle_ipc_reply, handle_ipc_send}, task::TaskRegisters, task_storage::get_task_by_index}}, define_per_cpu_u64, early_print, early_println};

mod ipc_handlers;

struct IpcSyscallArguments {
    ep_id: u64,
    msg: [u64; 4],
}

#[derive(Debug)]
struct SyscallArguments {
    syscall_number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
}


static LOCK: Mutex<()> = Mutex::new(());

fn syscall_dispatcher(registers: &mut TaskRegisters, args: &SyscallArguments) -> u64 {
    //debug

    if args.syscall_number == 0x10 {
        let _guard = LOCK.lock();
        let ptr = args.arg1 as *const u8;
        let len = args.arg2 as usize;

        if args.arg1 < 0x1000 || args.arg1 > 0x0000_7FFF_FFFF_FFFF {
            return 1;
        }
        if len > 4096 {
            return 1;
        }

        let slice = unsafe { core::slice::from_raw_parts(ptr, len) };

        for &byte in slice {
            if byte == 0 { break; }
            early_print!("{}", byte as char);
        }
        return 0;
    }

    let curr_task_id = PerCpuSchedulerData::get().curr_task_id.id();

    let ipc = IpcSyscallArguments {
            ep_id: args.arg1,
            msg: [args.arg2, args.arg3, args.arg4, args.arg5],
    };

    match args.syscall_number {
        x if x == IpcSyscallNumbers::IpcEpCreate as u64 => handle_ipc_ep_create(curr_task_id),

        x if x == IpcSyscallNumbers::IpcEpDestroy as u64 => handle_ipc_ep_destroy(curr_task_id) as u64,

        x if x == IpcSyscallNumbers::IpcSend as u64 => handle_ipc_send(curr_task_id, &ipc) as u64,

        x if x == IpcSyscallNumbers::IpcRecv as u64 => handle_ipc_recv(curr_task_id, args.arg1, registers) as u64,

        x if x == IpcSyscallNumbers::IpcCall as u64 => handle_ipc_call(curr_task_id, &ipc, registers) as u64,

        x if x == IpcSyscallNumbers::IpcReply as u64 => handle_ipc_reply(curr_task_id, &ipc) as u64,

        _ => {
            early_println!("Unknown syscall: {} task={}", args.syscall_number, curr_task_id);
            return 0;
        }
    }
}

pub fn init_syscall_subsystem() {
    set_per_cpu_USER_STACK_SCRATCH(0);
    unsafe {
        Efer::update(|efer| {
            *efer |= EferFlags::SYSTEM_CALL_EXTENSIONS;
        });
    }

    SFMask::write(RFlags::INTERRUPT_FLAG);

    let syscall_handler_addr = VirtAddr::new(syscall_handler as u64);
    LStar::write(syscall_handler_addr);
}

define_per_cpu_u64!(
    pub(super) TOP_OF_KERNEL_STACK
);

define_per_cpu_u64!(
    pub(super) USER_STACK_SCRATCH
);

#[unsafe(naked)]
pub(super) unsafe extern "C" fn syscall_handler() {
    naked_asm!(
        "swapgs",
        "mov gs:{user_stack_scratch}, rsp",
        "mov rsp, gs:{kernel_stack}",

        // iret frame
        "push {user_data_selector}",    // SS
        "push gs:{user_stack_scratch}", // RSP
        "push r11",                     // RFLAGS
        "push {user_code_selector}",    // CS
        "push rcx",                     // RIP

        "push rax",                     

        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rax",
        "push r8",
        "push r9",
        "push r10",
        "push r11",
        "push rbx",
        "push rbp",
        "push r12",
        "push r13",
        "push r14",
        "push r15",

        "mov rdi, rsp",
        "call {syscall_handler_inner}",

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

        "pop rax",

        "pop rcx",                     
        "add rsp, 8",                   
        "pop r11",                      
        "pop rsp",                      

        "swapgs",
        "sysretq",

        kernel_stack = sym TOP_OF_KERNEL_STACK,
        user_data_selector = const USER_DATA_SELECTOR.0,
        user_code_selector = const USER_CODE_SELECTOR.0,
        user_stack_scratch = sym USER_STACK_SCRATCH,
        syscall_handler_inner = sym syscall_handler_inner,
    )
}

extern "C" fn syscall_handler_inner(registers: &mut TaskRegisters) {
    let args = SyscallArguments {
        syscall_number: registers.syscall_number_or_irq_or_error_code,
        arg1: registers.rdi,
        arg2: registers.rsi,
        arg3: registers.rdx,
        arg4: registers.r10,
        arg5: registers.r8,
    };

    registers.syscall_number_or_irq_or_error_code = syscall_dispatcher(registers, &args);
}