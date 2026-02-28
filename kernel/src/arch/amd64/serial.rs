use core::fmt;

use lazy_static::lazy_static;
use spin::Mutex;
use uart_16550::SerialPort;

lazy_static! {
    pub static ref SERIAL1: Mutex<SerialPort> = {
        let mut serial = unsafe { SerialPort::new(0x3F8) };
        serial.init();
        Mutex::new(serial)
    };
}

#[doc(hidden)]
pub fn _print(args: fmt::Arguments) {
    use core::fmt::Write;
    let flags = x86_64::registers::rflags::read();
    x86_64::instructions::interrupts::disable();
    SERIAL1.lock().write_fmt(args).unwrap();
    if flags.contains(x86_64::registers::rflags::RFlags::INTERRUPT_FLAG) {
        x86_64::instructions::interrupts::enable();
    }
}
