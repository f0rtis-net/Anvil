use bitfield_struct::bitfield;
use x86_64::{VirtAddr, structures::paging::{Page, PageTableFlags, Size4KiB}};

use crate::{arch::amd64::{acpi::{get_acpi_tables, madt::MadTable}, memory::{misc::phys_to_virt, vmm::kmap_mmio_page}}, misc::registers::RegisterRW, register_struct};

const IOAPIC_ID_REGISTER_OFFSET: u8 = 0x00;

#[bitfield(u32)]
pub struct IOAPICID {
    #[bits(24)]
    __reserved: u32,
    #[bits(4)]
    pub id: u8,
    #[bits(4)]
    __reserved: u8,
}

const IOAPIC_REDIRECTION_TABLE_REGISTER_OFFSET: u8 = 0x10;

#[bitfield(u64)]
pub struct IOAPICRedirectionTableRegister {
    pub interrupt_vector: u8,
    #[bits(3)]
    pub delivery_mode: u8,
    pub destination_mode: bool,
    pub delivery_status: bool,
    pub interrupt_input_pin_polarity: bool,
    pub remote_irr: bool,
    pub trigger_mode: bool,
    pub interrupt_mask: bool,
    #[bits(39)]
    __reserved: u64,
    pub destination_field: u8,
}

register_struct! {
    IOApicRegisters {
        0x00 => io_reg_select: RegisterRW<u8>,
        0x10 => io_window: RegisterRW<u32>
    }
}

pub struct IOApic {
    id: u8,
    gsi_base: u32,
    registers: IOApicRegisters
}

impl IOApic {
    pub fn new() -> Self {
        let acpi = get_acpi_tables().read();
        let ioapic = acpi.get_table::<MadTable>().unwrap().ioapics.get(0).unwrap();
        
        let ioapic_converted = VirtAddr::new(phys_to_virt(ioapic.address.as_u64() as usize) as u64);

        let page = Page::<Size4KiB>::containing_address(ioapic_converted);
        let aligned_virt_addr = page.start_address();

        let flags = PageTableFlags::PRESENT 
            | PageTableFlags::WRITABLE 
            | PageTableFlags::NO_CACHE;
        
        kmap_mmio_page(aligned_virt_addr, ioapic.address, flags);

        let registers = unsafe { IOApicRegisters::from_address(aligned_virt_addr.as_u64() as usize) };

        Self {
            id: ioapic.id,
            gsi_base: ioapic.gsi_base,
            registers
        }
    }

    pub fn read_32b_from_reg(&self, reg: u8) -> u32 {
        self.registers.io_reg_select().write(reg);
        self.registers.io_window().read()
    }

    pub fn read_64b_from_reg(&self, reg: u8) -> u64 {
        let low = self.read_32b_from_reg(reg);
        let hight = self.read_32b_from_reg(reg + 1);
        (u64::from(hight) << 32) | u64::from(low)
    }

    pub fn write_32b_to_reg(&self, register: u8, value: u32) {
        self.registers.io_reg_select().write(register);
        self.registers.io_window().write(value);
    }

    pub fn write_64b_to_reg(&self, register: u8, value: u64) {
        let low = value as u32;
        let high = (value >> 32) as u32;
        self.write_32b_to_reg(register, low);
        self.write_32b_to_reg(register + 1, high);
    }

    pub fn ioapic_id(&self) -> IOAPICID {
        let raw = self.read_32b_from_reg(IOAPIC_ID_REGISTER_OFFSET);
        IOAPICID::from(raw)
    }

    pub fn write_ioredtbl(&self, entry: u8, value: IOAPICRedirectionTableRegister) {
        assert!(entry < 24, "Intel IOAPIC only has 24 entries!");
        let offset = IOAPIC_REDIRECTION_TABLE_REGISTER_OFFSET + (entry * 2);
        self.write_64b_to_reg(offset, value.into());
    }
}