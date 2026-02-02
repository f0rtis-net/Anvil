use x86_64::registers::control::Cr2;

use crate::{arch::amd64::cpu::hlt_loop, isr, early_println};

isr!(14, page_fault, |frame| {
    let fault_addr = Cr2::read().unwrap().as_u64();

    early_println!("Handled page fault at addr: {:#018x}", fault_addr);

    early_println!("{}", frame);

    early_println!(" Unhandled page fault sutiation, halting CPU");

    hlt_loop()
});