use x86_64::{PhysAddr, VirtAddr, structures::paging::{Page, PageTableFlags, Size4KiB}};

use crate::{arch::amd64::memory::vmm::kmap_mmio_page, misc::registers::{RegisterRO, RegisterRW, RegisterWO}, register_struct};

// flags for SVR (Spurious Interrupt Vector Register)
const SVR_ENABLE: u32 = 0x100;    // Enable APIC
const SVR_SUPPRESS_EOI_BROADCAST: u32 = 0x1000; // for x2APIC

// flags for ICR (Interrupt Command Register)
const ICR_DELIVERY_FIXED: u32 = 0x000;
const ICR_DELIVERY_INIT: u32 = 0x500;
const ICR_DELIVERY_STARTUP: u32 = 0x600;
const ICR_DEST_PHYSICAL: u32 = 0x000;
const ICR_DEST_LOGICAL: u32 = 0x800;
const ICR_LEVEL_ASSERT: u32 = 0x4000;
const ICR_TRIGGER_EDGE: u32 = 0x0000;
const ICR_TRIGGER_LEVEL: u32 = 0x8000;
const ICR_DEST_SELF: u32 = 0x40000;
const ICR_DEST_ALL: u32 = 0x80000;
const ICR_DEST_ALL_EX_SELF: u32 = 0xC0000;
const SPURIOUS_VECTOR: u32 = 0xEF;

#[repr(u32)]
pub enum LapicTimerDivide {
    Div1   = 0b1011,
    Div2   = 0b0000,
    Div4   = 0b0001,
    Div8   = 0b0010,
    Div16  = 0b0011,
    Div32  = 0b1000,
    Div64  = 0b1001,
    Div128 = 0b1010,
}

register_struct! {
    LAPICRegisters {
        0x020 => lapic_id: RegisterRO<u32>,
        0x030 => lapic_ver: RegisterRO<u32>,
        0x080 => lapic_tpr: RegisterRW<u32>,
        0x0B0 => lapic_eoi: RegisterWO<u32>,
        0x0F0 => lapic_svr: RegisterRW<u32>,
        0x280 => lapic_esr: RegisterRW<u32>,
        0x300 => lapic_icr_low: RegisterRW<u32>,
        0x310 => lapic_icr_high: RegisterRW<u32>,
        0x320 => lapic_timer: RegisterRW<u32>,
        0x330 => lapic_thermal: RegisterRW<u32>,
        0x340 => lapic_perf: RegisterRW<u32>,
        0x350 => lapic_lint0: RegisterRW<u32>,
        0x360 => lapic_lint1: RegisterRW<u32>,
        0x370 => lapic_lvt_err: RegisterRW<u32>,
        0x380 => lapic_timer_init: RegisterRW<u32>,
        0x390 => lapic_timer_curr: RegisterRW<u32>,
        0x3E0 => lapic_timer_div: RegisterRW<u32>,
    }
}

pub struct Lapic {
    registers: LAPICRegisters
}

unsafe impl Send for Lapic {}
unsafe impl Sync for Lapic {}

impl Lapic {
    pub fn new(phys_addr: PhysAddr, virt_addr: VirtAddr) -> Self {
        let page = Page::<Size4KiB>::containing_address(virt_addr);
        let aligned_virt_addr = page.start_address();

        let flags = PageTableFlags::PRESENT 
            | PageTableFlags::WRITABLE 
            | PageTableFlags::NO_CACHE;
        
        kmap_mmio_page(aligned_virt_addr, phys_addr, flags);
        let registers = unsafe { LAPICRegisters::from_address(aligned_virt_addr.as_u64() as usize) };

        Self {
            registers
        }
    }
}

impl Lapic {
    pub fn eoi(&self) {
        self.registers.lapic_eoi().write(0);
    }

    pub fn id(&self) -> u32 {
        self.registers.lapic_id().read() >> 24
    }

    pub fn enable(&self) {
        let mut svr = self.registers.lapic_svr().read();

        svr &= !0xFF;                    
        svr |= SPURIOUS_VECTOR;          
        svr |= SVR_ENABLE;               

        self.registers.lapic_svr().write(svr);
        self.registers.lapic_svr().read();
    }

    pub fn set_task_priority(&self, priority: u32) {
        self.registers.lapic_tpr().write(priority);
    }

    pub fn setup_timer_periodic(
        &self,
        vector: u8,
        div: LapicTimerDivide,
        initial_count: u32,
    ) {
        self.set_timer_divide(div);
        self.set_lvt_timer(vector, true);
        self.set_timer_initial(initial_count);
    }

    pub fn stop_timer(&self) {
        self.registers.lapic_timer_init().write(0);
    }

    pub fn set_timer_divide(&self, div: LapicTimerDivide) {
        self.registers.lapic_timer_div().write(div as u32);
    }

    pub fn set_lvt_timer(&self, vector: u8, periodic: bool) {
        let mut value = vector as u32;

        if periodic {
            value |= 1 << 17; 
        }

        self.registers.lapic_timer().write(value);
    }

    pub fn set_timer_initial(&self, count: u32) {
        self.registers.lapic_timer_init().write(count);
    }

    pub fn read_timer_current(&self) -> u32 {
        self.registers.lapic_timer_curr().read()
    }

    pub fn is_initialized(&self) -> bool {
        self.registers.address != 0
    }
}