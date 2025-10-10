#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(slice_as_array)]
#![feature(core_intrinsics)]

extern crate alloc;

use core::{panic::PanicInfo};

use bootloader::{entry_point, BootInfo};
use x86_64::{instructions};

use crate::{acpi::init_acpi, bdrivers::input::ps2_kb::init_keyboard, fs::test, gdt::init_gdt, interrupts::setup_idt, loader::test_init, memory::initialize_memory, port::{init_pics, setup_timer_freq}};
mod vga_buffer;
mod interrupts;
mod gdt;
mod acpi;
mod loader;
mod task;
mod port;
mod memory;
mod syscall;
mod fs;
mod bdrivers;

entry_point!(kernel_start);

fn kernel_start(boot_info: &'static BootInfo) -> ! {
    println!("Initializing idt...");
    setup_idt();

    println!("Initializing gdt...");
    init_gdt();

    let mut mem_result = initialize_memory(boot_info.physical_memory_offset, &boot_info.memory_map);

    println!("Initializing acpi...");
    init_acpi(mem_result.phys_mem_offset);

    test();

    test_init(&mut mem_result.frame_alloc,mem_result.phys_mem_offset);

    init_keyboard();

    init_pics();

    setup_timer_freq(1000); // 1000 HZ - 1 ms

    loop {
        instructions::hlt();
    }
}


#[panic_handler]
fn panic(_info: &PanicInfo) -> ! {
    println!("{}", _info);

    loop {
       instructions::hlt();
    }
}