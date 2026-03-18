use crate::arch::amd64::{cpu::hlt_loop, scheduler::sleep};

pub enum ThreadSyscallNums {
    ThreadSleep = 0x99,
    ThreadExit = 0x11
}

pub (crate) fn thread_exit(code: u64) -> ! {
    todo!()
}

pub (crate) fn thread_sleep(ns: u64) -> u64 {
    sleep(ns);

    0
}