use crate::arch::amd64::cpu::frames::InterruptFrame;

pub type Handler = extern "C" fn(&InterruptFrame);

#[repr(C)]
pub struct InterruptDescriptor {
    pub vector: u8,
    pub handler: Handler,
}

unsafe extern "C" {
    pub static __isr_table_start: InterruptDescriptor;
    pub static __isr_table_end: InterruptDescriptor;

    pub static __irq_table_start: InterruptDescriptor;
    pub static __irq_table_end: InterruptDescriptor;
}