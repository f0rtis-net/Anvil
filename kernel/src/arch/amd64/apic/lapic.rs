use x86_64::{PhysAddr, VirtAddr, structures::paging::{Page, PageTableFlags, Size4KiB}};

use crate::{arch::amd64::memory::vmm::{kmap_mmio_page}, irq};


// LAPIC registers (offsets from base addr)
const LAPIC_ID: u32 = 0x020;      // APIC ID
const LAPIC_VER: u32 = 0x030;     // APIC Version
const LAPIC_TPR: u32 = 0x080;     // Task Priority
const LAPIC_EOI: u32 = 0x0B0;     // End Of Interrupt
const LAPIC_SVR: u32 = 0x0F0;     // Spurious Interrupt Vector
const LAPIC_ESR: u32 = 0x280;     // Error Status
const LAPIC_ICR_LOW: u32 = 0x300; // Interrupt Command Low
const LAPIC_ICR_HIGH: u32 = 0x310;// Interrupt Command High
const LAPIC_TIMER: u32 = 0x320;   // LVT Timer
const LAPIC_THERMAL: u32 = 0x330; // LVT Thermal
const LAPIC_PERF: u32 = 0x340;    // LVT Performance
const LAPIC_LINT0: u32 = 0x350;   // LVT LINT0
const LAPIC_LINT1: u32 = 0x360;   // LVT LINT1
const LAPIC_ERROR: u32 = 0x370;   // LVT Error
const LAPIC_TIMER_INIT: u32 = 0x380;  // Timer Initial Count
const LAPIC_TIMER_CURR: u32 = 0x390;  // Timer Current Count
const LAPIC_TIMER_DIV: u32 = 0x3E0;   // Timer Divide Configuration

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

pub struct Lapic {
    base: *mut u32, 
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
        
        Self {
            base: aligned_virt_addr.as_mut_ptr(),
        }
    }
}

impl Lapic {
    #[inline(always)]
    pub fn write(&self, reg: u32, val: u32) {
        unsafe { core::ptr::write_volatile(self.base.add((reg / 4) as usize), val) }
    }

    #[inline(always)]
    pub fn read(&self, reg: u32) -> u32 {
        unsafe { core::ptr::read_volatile(self.base.add((reg / 4) as usize)) }
    }

    pub fn eoi(&self) {
        self.write(LAPIC_EOI, 0);
    }

    pub fn id(&self) -> u32 {
        self.read(LAPIC_ID) >> 24
    }

    pub fn enable(&self) {
        let mut svr = self.read(LAPIC_SVR);

        svr &= !0xFF;                    
        svr |= SPURIOUS_VECTOR;          
        svr |= SVR_ENABLE;               

        self.write(LAPIC_SVR, svr);
        self.read(LAPIC_SVR);
    }

    pub fn set_task_priority(&self, priority: u32) {
        self.write(LAPIC_TPR, priority);
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
        self.write(LAPIC_TIMER_INIT, 0);
    }

    pub fn set_timer_divide(&self, div: LapicTimerDivide) {
        self.write(LAPIC_TIMER_DIV, div as u32);
    }

    pub fn set_lvt_timer(&self, vector: u8, periodic: bool) {
        let mut value = vector as u32;

        if periodic {
            value |= 1 << 17; 
        }

        self.write(LAPIC_TIMER, value);
    }

    pub fn set_timer_initial(&self, count: u32) {
        self.write(LAPIC_TIMER_INIT, count);
    }

    pub fn read_timer_current(&self) -> u32 {
        self.read(LAPIC_TIMER_CURR)
    }

    pub fn is_initialized(&self) -> bool {
        !self.base.is_null()
    }
}