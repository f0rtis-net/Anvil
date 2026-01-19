use crate::{arch::{ArchInitInfo, amd64::{gdt::init_gdt, interrupts::idt::init_idt, memory::{MemoryInitInfo, init_memory_subsys}}}, serial_println};

pub mod serial;
pub mod cpu;
mod gdt;
mod interrupts;
mod ports;
mod pic;
mod memory;


pub fn init_arch(arch_info: ArchInitInfo) {
    serial_println!("Initializing amd64 arch...");

    serial_println!("Initializing GDT...");
    init_gdt();
    serial_println!("GDT initialized!");

    serial_println!("Initializing IDT...");
    init_idt();
    serial_println!("IDT Initialized!");

    serial_println!("Initializing memory subsystem...");
    init_memory_subsys(MemoryInitInfo {
        hhdm_offset: arch_info.hhdm_offset,
        memmap_entry: arch_info.memmap_entry
    });
    serial_println!(" Memory subsystem initialized!");
}

