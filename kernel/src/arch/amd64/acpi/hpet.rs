use acpi::{PhysicalMapping, sdt::hpet::HpetTable};
use x86_64::PhysAddr;

use crate::arch::amd64::acpi::{main_table_parser::MainTableParser, parsed_table::AcpiParsedTable};

pub struct HpetTableParsed {
    pub base_address: PhysAddr,
    pub clock_tick_unit: u16,
    pub page_protection: u8,
    pub hpet_number: u8,
}

impl AcpiParsedTable for HpetTableParsed {
    type Raw = HpetTable;
    
    fn parse(mapping: PhysicalMapping<MainTableParser, Self::Raw>) -> Self {
        let table = mapping.get();

        let gas: acpi::address::RawGenericAddress = table.base_address;

        let phys = PhysAddr::new(gas.address);

        // HPET spec: base must be 64-bit aligned
        assert!(
            phys.as_u64() & 0x7 == 0,
            "HPET base address is not aligned"
        );

        let page_protection = table.page_protection_and_oem & 0b1_1111;

        HpetTableParsed {
            base_address: phys,
            hpet_number: table.hpet_number,
            clock_tick_unit: table.clock_tick_unit,
            page_protection,
        }
    }
}