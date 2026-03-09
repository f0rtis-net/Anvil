use x86_64::registers::control::{Cr2, Cr3};

use crate::{arch::amd64::{cpu::hlt_loop}, early_println, isr};


isr!(14, page_fault, |frame| {
    let fault_addr = Cr2::read().unwrap().as_u64();
    
    early_println!("Fault CR3: {:#x}", Cr3::read().0.start_address().as_u64());

    early_println!("Handled page fault at addr: {:#018x}", fault_addr);

    early_println!("{}", frame);

    early_println!(" Unhandled page fault sutiation, halting CPU");

    hlt_loop()
});