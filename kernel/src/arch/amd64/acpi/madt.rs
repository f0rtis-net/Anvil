use acpi::{PhysicalMapping, sdt::madt::{Madt, MadtEntry}};
use alloc::vec::Vec;
use x86_64::PhysAddr;

use crate::arch::amd64::acpi::{main_table_parser::MainTableParser, parsed_table::AcpiParsedTable};

pub struct MadtCpuInfo {
    pub lapic_id: u8
}

pub struct MadtIoApicInfo {
    pub id: u8,
    pub address: PhysAddr,
    pub gsi_base: u32,
}

pub struct MadtIrqOverride {
    pub irq: u8,
    pub gsi: u32,
    pub flags: u16,
}

pub struct MadTable {
    pub lapic_addr: PhysAddr,
    pub cpus: Vec<MadtCpuInfo>,
    pub ioapics: Vec<MadtIoApicInfo>,
    pub irq_overrides: Vec<MadtIrqOverride>,
}

impl AcpiParsedTable for MadTable {
    type Raw = Madt;

    fn parse(mapping: PhysicalMapping<MainTableParser, Self::Raw>) -> Self {
        let madt = mapping.get();

        let mut cpus = Vec::new();
        let mut ioapics = Vec::new();
        let mut irq_overrides = Vec::new();

        for entry in madt.entries() {
            match entry {
                MadtEntry::LocalApic(p) if p.flags & 1 != 0 => {
                    cpus.push(MadtCpuInfo {
                        lapic_id: p.apic_id,
                    });
                }

                MadtEntry::LocalX2Apic(p) if p.flags & 1 != 0 => {
                    cpus.push(MadtCpuInfo {
                        lapic_id: p.x2apic_id as u8,
                    });
                }

                MadtEntry::IoApic(io) => {
                    ioapics.push(MadtIoApicInfo {
                        id: io.io_apic_id,
                        address: PhysAddr::new(io.io_apic_address as u64),
                        gsi_base: io.global_system_interrupt_base,
                    });
                }

                MadtEntry::InterruptSourceOverride(iso) => {
                    irq_overrides.push(MadtIrqOverride {
                        irq: iso.irq,
                        gsi: iso.global_system_interrupt,
                        flags: iso.flags,
                    });
                }

                _ => {}
            }
        }

        Self {
            lapic_addr: PhysAddr::new(madt.local_apic_address as u64),
            cpus,
            ioapics,
            irq_overrides
        }
    }
}