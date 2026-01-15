use paste::paste;
use crate::{arch::amd64::{cpu::frames::InterruptFrame, hlt_loop, interrupts::{idt::{IDT_COUNT, ISR_COUNT}, tables::InterruptDescriptor}}, serial_println};

macro_rules! isr {
    ($vec:literal, $name:ident, |$stack:ident| $body:block) => {
        const _: () = {
            assert!($vec < ISR_COUNT, "ISR vector must be in range 0..31");
        };

        paste! {
            extern "C" fn $name($stack: &InterruptFrame) {
                $body
            }

            #[used]
            #[unsafe(link_section = ".isr_table")]
            static [<$name:upper _ISR>]: InterruptDescriptor = InterruptDescriptor {
                vector: $vec,
                handler: $name,
            };
        }
    };
}

macro_rules! irq {
    ($vec:literal, $name:ident, |$stack:ident| $body:block) => {
        const _: () = {
            assert!(
                ($vec as usize) + ISR_COUNT < IDT_COUNT,
                "IRQ vector out of IDT range"
            );
        };

        paste! {
            extern "C" fn $name($stack: &InterruptFrame) {
                $body
            }

            #[used]
            #[unsafe(link_section = ".irq_table")]
            static [<$name:upper _IRQ>]: InterruptDescriptor = InterruptDescriptor {
                vector: (ISR_COUNT + ($vec as usize)) as u8,
                handler: $name,
            };
        }
    };
}

isr!(3, int3, |stack| {
    serial_println!("Custom int3 handler:\n{}", stack);
});