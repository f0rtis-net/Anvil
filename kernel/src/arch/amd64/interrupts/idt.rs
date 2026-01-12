use x86_64::{VirtAddr, instructions::{self, tables::lidt}, registers::segmentation::{CS, Segment}, structures::{DescriptorTablePointer, gdt::SegmentSelector}};
use lazy_static::lazy_static;

use crate::arch::amd64::interrupts::base::init_dispatch_from_sections;

pub const IDT_COUNT: usize = 256;
pub const ISR_COUNT: usize = 32;

pub type IdtTable = [IDTEntry; IDT_COUNT];

unsafe extern "C" {
static interrupts_stub_table: [u64; IDT_COUNT];
}

#[repr(C, packed)]
struct InterruptDescriptorTable(IdtTable);

impl InterruptDescriptorTable {
    fn load(&'static self) {
        let idt_ptr = DescriptorTablePointer {
            base:  VirtAddr::from_ptr(self as *const _),
            limit: (size_of::<Self>() - 1) as u16,
        };
        unsafe {
            lidt(&idt_ptr);
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
pub struct IDTEntry {
    handler_low: u16,
    gdt_selector: u16,
    options: u16,
    handler_mid: u16,
    handler_hi: u32,
    reserved: u32,
}

impl IDTEntry {
    pub fn new(
        handler: *const (),
        gdt_selector: SegmentSelector,
        int_stack_idx: u8,
        disable_interrupts: bool,
        dpl_priv: u8,
    ) -> IDTEntry {
        let mut options: u16 = int_stack_idx as u16 & 0b111;
        if !disable_interrupts {
            options |= 1 << 8;
        }
        options |= 1 << 9;
        options |= 1 << 10;
        options |= 1 << 11;
        options |= (dpl_priv as u16 & 0b11) << 13;
        options |= 1 << 15;
        let handler_ptr = handler as u64;
        let handler_low = (handler_ptr & 0xFFFF) as u16;
        let handler_mid = ((handler_ptr >> 16) & 0xFFFF) as u16;
        let handler_hi = (handler_ptr >> 32) as u32;
        let gdt_selector = gdt_selector.0;
        IDTEntry {
            handler_low,
            handler_mid,
            handler_hi,
            options,
            gdt_selector,
            reserved: 0,
        }
    }

    fn empty() -> IDTEntry {
        IDTEntry {
            handler_low: 0,
            handler_mid: 0,
            handler_hi: 0,
            options: 0,
            gdt_selector: CS::get_reg().0,
            reserved: 0,
        }
    }
}

lazy_static! {
    static ref INTERRUPT_TABLE: InterruptDescriptorTable = {
        let mut vectors = [IDTEntry::empty(); IDT_COUNT];
        
        for i in 0..IDT_COUNT {
            let handler = unsafe { interrupts_stub_table[i] } as *const ();
            vectors[i] = IDTEntry::new(
                handler,
                CS::get_reg(),
                0,
                true,
                0,
            );
        }

        init_dispatch_from_sections();

        InterruptDescriptorTable(vectors)
    };
}

pub fn init_idt() {
    instructions::interrupts::disable();
    INTERRUPT_TABLE.load();
    instructions::interrupts::enable();
}