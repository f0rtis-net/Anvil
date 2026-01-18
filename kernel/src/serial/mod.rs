use core::fmt;

pub fn print(args: fmt::Arguments) {
    crate::arch::current::serial::_print(args);
}

#[macro_export]
macro_rules! serial_print {
    ($($arg:tt)*) => {
        $crate::serial::print(format_args!($($arg)*))
    };
}

#[macro_export]
macro_rules! serial_println {
    () => ($crate::serial_print!("\n"));
    ($($arg:tt)*) => {
        eclipse_framebuffer::print!("{}\n", format_args!($($arg)*));
        $crate::serial_print!("{}\n", format_args!($($arg)*))
    };
}
