use crate::{loader::SCHEDULER, println};

pub struct SyscallUsableRegs {
    pub rax: u64, 

    pub rdi: u64, rsi: u64, rdx: u64,
    pub r10: u64, r9: u64, r8: u64
}

fn exit_task(exit_code: i64) {
    println!("called syscall: exit task");
    SCHEDULER.remove_current_from_schedule(exit_code);
}

fn kill_task(pid: u64, exit_code: i64) {
    println!("called syscall: kill task");
    SCHEDULER.remove_task_from_schedule(pid as usize, exit_code);
}


fn get_pid(regs: *mut SyscallUsableRegs) {
    println!("called syscall: get pid");
    unsafe {
        (*regs).rax = SCHEDULER.get_curr_pid() as u64;
    }
}

pub unsafe extern "sysv64" fn syscall_handler(regs: *mut SyscallUsableRegs) {
    match (*regs).rax {
        0x27 => get_pid(regs),
        0x3c => exit_task((*regs).rdi as i64),
        0x3e => kill_task((*regs).rdi, (*regs).rsi as i64),
        _ => panic!("Invalid syscall function number: 0x{:x}", (*regs).rax)
    }
}