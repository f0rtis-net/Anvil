#![no_std]
#![no_main]
#![feature(abi_x86_interrupt)]
#![feature(generic_const_exprs)]

extern crate alloc;

use limine::BaseRevision;
use limine::request::{FramebufferRequest, HhdmRequest, MemoryMapRequest, RequestsEndMarker, RequestsStartMarker};

use crate::arch::{ArchInitInfo, arch_init, hlt_loop};

mod arch;
mod serial;

/// Sets the base revision to the latest revision supported by the crate.
/// See specification for further info.
/// Be sure to mark all limine requests with #[used], otherwise they may be removed by the compiler.
#[used]
// The .requests section allows limine to find the requests faster and more safely.
#[unsafe(link_section = ".requests")]
static BASE_REVISION: BaseRevision = BaseRevision::new();

#[used]
#[unsafe(link_section = ".requests")]
static FRAMEBUFFER_REQUEST: FramebufferRequest = FramebufferRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static MEMMAP_INFO: MemoryMapRequest = MemoryMapRequest::new();

#[used]
#[unsafe(link_section = ".requests")]
static HHDM_OFFSET: HhdmRequest = HhdmRequest::new();


/// Define the stand and end markers for Limine requests.
#[used]
#[unsafe(link_section = ".requests_start_marker")]
static _START_MARKER: RequestsStartMarker = RequestsStartMarker::new();
#[used]
#[unsafe(link_section = ".requests_end_marker")]
static _END_MARKER: RequestsEndMarker = RequestsEndMarker::new();

#[unsafe(no_mangle)]
unsafe extern "C" fn kmain() -> ! {
    assert!(BASE_REVISION.is_supported());

    serial_println!("Hello from rust!");

    arch_init(ArchInitInfo {
        hhdm_offset: HHDM_OFFSET.get_response().unwrap().offset(),
        memmap_entry: MEMMAP_INFO.get_response().unwrap().entries()
    });

    hlt_loop();
}

#[panic_handler]
fn rust_panic(_info: &core::panic::PanicInfo) -> ! {
    serial_println!("KERNEL WAS CRASHED!. Message: {:?}", _info.message());
    hlt_loop();
}
