use core::arch::naked_asm;

use x86_64::{VirtAddr, registers::{control::{Efer, EferFlags}, model_specific::{LStar, SFMask}, rflags::RFlags}};

use crate::{arch::amd64::{gdt::{USER_CODE_SELECTOR, USER_DATA_SELECTOR}, scheduler::task::TaskRegisters}, define_per_cpu_u64, early_println, irq};

struct SyscallArguments {
    syscall_number: u64,
    arg1: u64,
    arg2: u64,
    arg3: u64,
    arg4: u64,
    arg5: u64,
    arg6: u64
}

fn syscall_dispatcher(args: &SyscallArguments) {
    early_println!("Called syscall dispatcher. Syscall number: {}", args.syscall_number);
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
        arg6: registers.r9
    };

    early_println!("Called new syscall");
    syscall_dispatcher(&args);
}

irq!(0x80, old_syscall_handler, |stack| {
    let args = SyscallArguments {
        syscall_number: stack.rax,
        arg1: stack.rdi,
        arg2: stack.rsi,
        arg3: stack.rdx,
        arg4: stack.r10,
        arg5: stack.r8,
        arg6: stack.r9
    };

    early_println!("Called old syscall");
    syscall_dispatcher(&args);
});