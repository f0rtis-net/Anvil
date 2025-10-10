use core::arch::{asm, naked_asm};
use crate::bdrivers::input::ps2_kb::handle_kb_irq;
use crate::port::end_of_interrupt;
use crate::syscall::syscall_handler;
use crate::{eprintln, gdt};
use crate::loader::context_switch;
use crate::{println};
use lazy_static::lazy_static;


use x86_64::registers::control::Cr2;
use x86_64::registers::segmentation::Segment;
use x86_64::registers::segmentation::CS;
use x86_64::instructions::tables::{lidt, DescriptorTablePointer};
use x86_64::structures::gdt::SegmentSelector;
use x86_64::structures::idt::InterruptStackFrame;


use core::mem::size_of;
use x86_64::{VirtAddr};

type IDTHandler = extern "x86-interrupt" fn();

extern "x86-interrupt" fn div_by_zero(stack_frame: &mut InterruptStackFrame) {
    eprintln!(" !! div by zero {:?}", stack_frame);
}

extern "x86-interrupt" fn breakpoint(stack_frame: &mut InterruptStackFrame) {
    eprintln!(" !! int3 {:?}", stack_frame);
}

extern "x86-interrupt" fn page_fault(stack_frame: &mut InterruptStackFrame, err_code: u64) {
    eprintln!(" !! page fault @ {:x}! err code: {} {:?}", stack_frame.instruction_pointer.as_u64(), err_code, stack_frame);
    println!("addr: {:#x}", Cr2::read());
    loop {}
}

extern "x86-interrupt" fn gpf(stack_frame: &mut InterruptStackFrame, err_code: u64) {
    eprintln!(" !! gpf! err code: {} {:?}", err_code, stack_frame);
    loop {}
}

extern "x86-interrupt" fn double_fault(stack_frame: &mut InterruptStackFrame, err_code: u64) {
    eprintln!(" !! double fault! err code: {} {:?}", err_code, stack_frame);
    loop {}
}

extern "x86-interrupt" fn ide(_stack_frame: &mut InterruptStackFrame) {
    eprintln!(" # IDE IRQ");
}

#[unsafe(naked)]
pub unsafe extern "sysv64" fn syscall(_stack_frame: &mut InterruptStackFrame) {
    naked_asm!(
        "
        push r8
        push r9
        push r10
        push rdx
        push rsi
        push rdi
        push rax

        mov rdi, rsp

        call {syscall_handler}

        pop rax
        pop rdi
        pop rsi
        pop rdx
        pop r10
        pop r9
        pop r8

        iretq
        ",
        syscall_handler = sym syscall_handler,
    )
}

#[unsafe(naked)]
unsafe extern "sysv64" fn timer(_stack_frame: &mut InterruptStackFrame) {
    naked_asm!("\
    push r15; push r14; push r13; push r12; push r11; push r10; push r9;\
    push r8; push rdi; push rsi; push rdx; push rcx; push rbx; push rax; push rbp;\
    mov rdi, rsp   // first arg of context switch is the context which is all the registers saved above
    sub rsp, 0x400
    jmp {context_switch}
    ", context_switch = sym context_switch);
}

macro_rules! irq_fn {
    ($f: ident, $i: literal, $e:expr) => {
        unsafe extern "x86-interrupt" fn $f(_stack_frame: &mut InterruptStackFrame) {
            asm!("cli");
            $e();
             end_of_interrupt($i);
            asm!("sti");
        }
    }
}

irq_fn!(kb_interrupt, 33, || {
    handle_kb_irq();
});

lazy_static! {
    static ref INTERRUPT_TABLE: InterruptDescriptorTable = {
        let mut vectors = [IDTEntry::empty(); 0x100];
        macro_rules! idt_entry {
            ($i:literal, $e:expr) => {
                vectors[$i] =
                    IDTEntry::new($e as *const IDTHandler, CS::get_reg(), 0, true, 0);
            };
        }
        idt_entry!(0, div_by_zero);
        idt_entry!(3, breakpoint);
        vectors[8] = IDTEntry::new(
            double_fault as *const IDTHandler,
            CS::get_reg(),
            (gdt::DOUBLE_FAULT_IST_INDEX + 1).try_into().unwrap(),
            true,
            0,
        );
        idt_entry!(13, gpf);
        idt_entry!(14, page_fault);
        idt_entry!(32, timer);
        idt_entry!(46, ide);
        idt_entry!(33, kb_interrupt);
        vectors[0x80] = IDTEntry::new(
            syscall as *const IDTHandler,
            CS::get_reg(),
            0,
            true,
            3,
        );

        InterruptDescriptorTable(vectors)
    };
}

#[repr(C, packed)]
struct InterruptDescriptorTable([IDTEntry; 0x100]);

impl InterruptDescriptorTable {
    fn load(&'static self) {
        let idt_ptr = DescriptorTablePointer {
            base:  VirtAddr::from_ptr(self as *const _),
            limit: (size_of::<Self>() - 1) as u16,
        };
        println!(" - Setting up IDT with {} entries", INTERRUPT_TABLE.0.len());
        println!(" - IDT ptr address: {:x}", &idt_ptr as *const _ as u64);
        println!(
            " - IDT address: {:x}",
            &INTERRUPT_TABLE.0 as *const _ as u64
        );
        unsafe {
            lidt(&idt_ptr);
        }
    }
}

#[derive(Copy, Clone)]
#[repr(C, packed)]
struct IDTEntry {
    handler_low: u16,
    gdt_selector: u16,
    options: u16,
    handler_mid: u16,
    handler_hi: u32,
    reserved: u32,
}

impl IDTEntry {
    fn new(
        handler: *const IDTHandler,
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

pub fn setup_idt() {
    INTERRUPT_TABLE.load();
}