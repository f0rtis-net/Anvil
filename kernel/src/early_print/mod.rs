pub mod fb_printer;

#[macro_export]
macro_rules! early_print {
    ($($arg:tt)*) => {{
        use core::fmt::Write;

        if let Some(renderer) = $crate::early_print::fb_printer::RENDERER.get() {
            let mut guard = renderer.lock();
            let _ = write!(guard, $($arg)*);
        }
    }};
}

#[macro_export]
macro_rules! early_println {
    () => {
        $crate::early_print!("\n");
        $crate::serial_print!("\n");
    };
    ($($arg:tt)*) => {{
        //serial output. TODO: Make config param, to enable / disable serial logging
        $crate::serial_print!($($arg)*);
        $crate::serial_print!("\n");

        //fb output
        $crate::early_print!($($arg)*);
        $crate::early_print!("\n");
    }};
}

