#[macro_export]
macro_rules! isr {
    ($vec:literal, $name:ident, |$stack:ident| $body:block) => {
        const _: () = {
            assert!(
                $vec < $crate::arch::amd64::interrupts::idt::ISR_COUNT,
                "ISR vector must be in range 0..31"
            );
        };

        paste::paste! {
            extern "C" fn $name($stack: &$crate::arch::amd64::cpu::frames::InterruptFrame) {
                $body
            }

            #[used]
            #[unsafe(link_section = ".isr_table")]
            static [<$name:upper _ISR>]: $crate::arch::amd64::interrupts::tables::InterruptDescriptor =
                $crate::arch::amd64::interrupts::tables::InterruptDescriptor {
                    vector: $vec,
                    handler: $name,
                };
        }
    };
}

#[macro_export]
macro_rules! irq {
    ($vec:literal, $name:ident, |$stack:ident| $body:block) => {
        const _: () = {
            assert!(
                ($vec as usize) < $crate::arch::amd64::interrupts::idt::IDT_COUNT && ($vec as usize) >= $crate::arch::amd64::interrupts::idt::ISR_COUNT,
                "IRQ vector out of IDT range"
            );
        };

        paste::paste! {
            extern "C" fn $name($stack: &$crate::arch::amd64::cpu::frames::InterruptFrame) {
                $body
            }

            #[used]
            #[unsafe(link_section = ".irq_table")]
            static [<$name:upper _IRQ>]: $crate::arch::amd64::interrupts::tables::InterruptDescriptor =
                $crate::arch::amd64::interrupts::tables::InterruptDescriptor {
                    vector: $vec as u8,
                    handler: $name,
                };
        }
    };
}