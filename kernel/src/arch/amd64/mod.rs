use x86_64::instructions;

use crate::{arch::amd64::{gdt::init_gdt, interrupts::idt::init_idt, pic::{init_pics, setup_timer_freq}}, serial_println};

pub mod serial;
mod gdt;
mod interrupts;
mod cpu;
mod ports;
mod pic;

pub fn init_arch() {
    serial_println!("Hello form amd64!");

    init_gdt();
    serial_println!("GDT Initialized!");

    init_idt();
    serial_println!("IDT Initialized!");

    init_pics();

    instructions::interrupts::int3();

    setup_timer_freq(1000);
}

pub fn hlt_loop() -> ! {
    loop {
        
    }
}
