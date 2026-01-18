use x86_64::instructions;

use crate::{arch::{ArchInitInfo, amd64::{gdt::init_gdt, interrupts::idt::init_idt, memory::{MemoryInitInfo, init_memory_subsys, pmm::abstract_allocator::{KmallocFlags, kfree, kmalloc}}}}, serial_println};

pub mod serial;
pub mod cpu;
mod gdt;
mod interrupts;
mod ports;
mod pic;
mod memory;

struct Test {
    a: u64,
    b: usize
}

pub fn init_arch(arch_info: ArchInitInfo) {
    serial_println!("Hello form amd64!");

    init_gdt();
    serial_println!("GDT Initialized!");

    init_idt();
    serial_println!("IDT Initialized!");

    instructions::interrupts::int3();

    serial_println!("Continue executing");

    serial_println!("Hhdm offset: {:#018x}", arch_info.hhdm_offset);

    init_memory_subsys(MemoryInitInfo {
        hhdm_offset: arch_info.hhdm_offset,
        memmap_entry: arch_info.memmap_entry
    });

    let bytes = kmalloc(size_of::<Test>(), KmallocFlags::Zeroed | KmallocFlags::Kernel);

    unsafe {
        let _struct = bytes as *mut Test;

        (*_struct).a = 10;
        (*_struct).b = 145;

        serial_println!("{} | {}", (*_struct).a, (*_struct).b);
    }

    serial_println!("Allocated memory for struct at addr: {:#018x}", bytes);

    kfree(bytes);
}

