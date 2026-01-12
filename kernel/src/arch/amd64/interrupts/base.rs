use crate::{arch::amd64::{cpu::frames::InterruptFrame, hlt_loop, interrupts::{idt::{IDT_COUNT, ISR_COUNT}, tables::{__irq_table_end, __irq_table_start, __isr_table_end, __isr_table_start, Handler, InterruptDescriptor}}, pic::eoi}, serial_println};

static mut HANDLERS: [Option<Handler>; IDT_COUNT] = [None; IDT_COUNT];

pub fn init_dispatch_from_sections() {
    unsafe {
        register_range(&__isr_table_start, &__isr_table_end);
        register_range(&__irq_table_start, &__irq_table_end);
    }
}

fn register_range(start: *const InterruptDescriptor, end: *const InterruptDescriptor) {
    let mut cur = start;
    while cur < end {
        unsafe {
            let d = &*cur;
            HANDLERS[d.vector as usize] = Some(d.handler);
            cur = cur.add(1);
        }
    }
}

#[unsafe(no_mangle)]
extern "C" fn base_trap(stack_frame: *const InterruptFrame) {
    let frame = unsafe { &*stack_frame };

    let vec = frame.interrupt as usize;

    let handler = unsafe { HANDLERS[vec] };
    if let Some(h) = handler {
        h(&frame);
        return;
    } 

    if (frame.interrupt as usize) < ISR_COUNT {
        serial_println!("Unhandled isr interrupt!\n {}", frame);
        hlt_loop();
    }

    serial_println!("Unhandled irq interrupt!\n {}", frame);
    eoi(((frame.interrupt as usize) + ISR_COUNT) as u8);
}