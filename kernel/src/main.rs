#![no_std]
#![no_main]
#![feature(cell_leak)]
#![feature(abi_x86_interrupt)]


use core::ptr;

use alloc::sync::Arc;

use crate::arch::amd64::cpu::smp::startup::init_bsp_core_smp;
use crate::arch::amd64::scheduler::exec_loader::make_init_task;
use crate::arch::amd64::scheduler::task_storage::add_task_to_execute;
use crate::arch::{arch_init, hlt_loop};
use crate::bootinfo::BootInfo;
use crate::cpio_parser::cpio_find;
use crate::early_print::fb_printer::ScrollingFbTextRenderer;
use crate::framebuffer::Framebuffer;
extern crate alloc;

mod arch;
mod serial;
mod selftest;
mod cmd_args;
mod framebuffer;
mod early_print;
mod bootinfo;
mod misc;
mod cpio_parser;

include!(concat!(env!("OUT_DIR"), "/kernel_version.rs"));

static FONT: &[u8] = include_bytes!("../external/cp850-8x16.psf");

pub fn print_hello_banner() {
    early_println!("");
    early_println!("=================================================");
    early_println!("  {} — experimental operating system", KERNEL_NAME);
    early_println!("-------------------------------------------------");
    early_println!("  Version:   {}", KERNEL_VERSION_FULL);
    early_println!("  Git:       {} ({})", GIT_HASH, GIT_BRANCH);
    early_println!("  Built:     unix {}", BUILD_UNIX_TIME);
    early_println!("  Toolchain: {}", RUSTC_VERSION);
    early_println!("  Target:    {}", TARGET_TRIPLE);
    early_println!("=================================================");
    early_println!("");
}

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    
    BootInfo::init();
    
    assert!(BootInfo::get().bootloader_supported());
    
    Framebuffer::init(
        BootInfo::get().framebuffer().unwrap().addr(), 
        BootInfo::get().framebuffer().unwrap().width() as usize, 
        BootInfo::get().framebuffer().unwrap().height() as usize, 
        BootInfo::get().framebuffer().unwrap().pitch() as usize,
        BootInfo::get().framebuffer().unwrap().bpp() as usize
    );

    ScrollingFbTextRenderer::init(
        FONT,
        Framebuffer::get_global()
    );
    
    print_hello_banner();

    arch_init();

    early_println!("Loading init service...");

    let init_srvs = BootInfo::get_init_srvs().expect("No init pack of services found!");
    let cpio_ptr = ptr::addr_of!(init_srvs);
    if let Some(data) = cpio_find(init_srvs, "server.bin") {
        let init = make_init_task(data, 1, cpio_ptr as u64).unwrap();
        add_task_to_execute(Arc::new(init));
        early_println!("Init service loaded!");
    } else {
        panic!("No init service found!");
    }

    early_println!("Post init arch...");
    init_bsp_core_smp();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    early_println!("KERNEL WAS CRASHED!. Message: {:?}", _info.message());
    hlt_loop();
}
