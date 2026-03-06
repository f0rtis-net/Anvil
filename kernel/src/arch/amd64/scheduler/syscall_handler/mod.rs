use core::arch::naked_asm;

use x86_64::{VirtAddr, registers::{control::{Efer, EferFlags}, model_specific::{LStar, SFMask}, rflags::RFlags}};

use crate::{arch::amd64::{gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR}, ipc::{IPC_MANAGER, IpcResult, endpoint::EndpointId, message::{FastMessage, MsgLabel}}, scheduler::{block_current_ipc, direct_transfer_ipc, get_per_cpu_no_guard_CURR_TASK_ID, task::TaskRegisters, task_storage::inject_sleeping_task}}, define_per_cpu_u64, early_print, early_println, irq};

struct IpcSyscallArguments {
    ep_id: u64,
    msg: [u64; 4],
}

struct SyscallArguments {
    syscall_number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
}

enum IpcSyscallNums {
    IPC_SEND = 0x60,
    IPC_RECV = 0x61,
    IPC_CALL = 0x62,
    IPC_REPLY = 0x63,
    IPC_EP_CREATE = 0x64,
    IPC_EP_DESTROY = 0x65,
}

/// Returns the IPC error code (0 = ok, 1 = error) for normal (sysretq) returns.
/// For BlockCurrent the function diverges and never returns to the caller.
fn syscall_dispatcher(registers: &mut TaskRegisters, args: &SyscallArguments) -> u64 {
    let curr_task_id = get_per_cpu_no_guard_CURR_TASK_ID();

    if args.syscall_number == 0x10 {
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

    match args.syscall_number {
        x if x == IpcSyscallNums::IPC_EP_CREATE as u64 => {
            let ep_id = IPC_MANAGER.lock().create_endpoint().unwrap().0;
            return ep_id;
        },

        x if x == IpcSyscallNums::IPC_SEND as u64 => {
            let ipc = IpcSyscallArguments {
                ep_id: args.arg1,
                msg: [args.arg2, args.arg3, args.arg4, args.arg5],
            };
            let msg    = FastMessage::with_data(MsgLabel::NOTIFY, ipc.msg);
            let result = IPC_MANAGER.lock().handle_send(
                curr_task_id,
                EndpointId::new(ipc.ep_id),
                msg,
            );

            match result {
                IpcResult::DirectTransfer { receiver, message } => {
                    direct_transfer_ipc(receiver, &message);
                    return 0;
                }

                IpcResult::BlockCurrent => {
                    registers.r9 = 0;
                    block_current_ipc(registers);
                }

                IpcResult::Error(_) => return 1,
                _ => return 0,
            }
        },

        x if x == IpcSyscallNums::IPC_RECV as u64 => {
            let ipc = IpcSyscallArguments {
                ep_id: args.arg1,
                msg: [args.arg2, args.arg3, args.arg4, args.arg5],
            };
            let result = IPC_MANAGER.lock().handle_recv(
                curr_task_id,
                EndpointId::new(ipc.ep_id),
            );

            match result {
                IpcResult::WakeSender { sender, message } => {
                    registers.rdi = message.label.0;
                    registers.rsi = message.data[0];
                    registers.rdx = message.data[1];
                    registers.r10 = message.data[2];
                    registers.r8  = message.data[3];
                    inject_sleeping_task(sender);
                    return 0;
                }

                IpcResult::BlockCurrent => {
                    registers.r9 = 0;
                    block_current_ipc(registers);
                }

                IpcResult::Error(_) => return 1,
                _ => return 0,
            }
        },

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

    let syscall_handler_addr = VirtAddr::new(syscall_handler as usize as u64);
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

        "push {user_data_selector}",    
        "push gs:{user_stack_scratch}", 
        "push r11",                     
        "push {user_code_selector}",    
        "push rcx",                     

        "push r15", 
        "push r14",
        "push r13",
        "push r12",
        "push r11",
        "push r10",
        "push r9",
        "push r8",
        "push rdi",
        "push rsi",
        "push rdx",
        "push rcx",
        "push rbx",
        "push rax",
        "push rbp",

        "mov rdi, rsp",
        "call {syscall_handler_inner}",

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

        "pop rcx",  
        "add rsp, 8", 
        "pop r11",  
        "pop rax",  
        "add rsp, 8", 

        "mov gs:{kernel_stack}, rsp",
        "mov rsp, rax",
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
        syscall_number: registers.rax,
        arg1: registers.rdi,
        arg2: registers.rsi,
        arg3: registers.rdx,
        arg4: registers.r10,
        arg5: registers.r8,
    };

    // syscall_dispatcher either returns a u64 result code (normal sysretq path)
    // or diverges via block_current_ipc (context-switch path).
    registers.r9 = syscall_dispatcher(registers, &args);
}