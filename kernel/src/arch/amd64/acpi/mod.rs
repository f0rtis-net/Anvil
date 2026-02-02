use core::any::{Any, TypeId};

use acpi::AcpiTables;
use alloc::{boxed::Box, collections::btree_map::BTreeMap};
use limine::memory_map::{Entry, EntryType};
use spin::{Once, RwLock};
use x86_64::{PhysAddr, VirtAddr, structures::paging::PageTableFlags};

use crate::arch::amd64::{acpi::{hpet::HpetTableParsed, madt::MadTable, main_table_parser::MainTableParser, parsed_table::AcpiParsedTable}, memory::{misc::{align_down, align_up}, pmm::HHDM_OFFSET, vmm::{PAGE_SIZE, kmap_page}}};

mod parsed_table;
mod main_table_parser;
pub mod madt;
pub mod hpet;

static ACPI_CTX: Once<RwLock<AcpiContext>> = Once::new();

pub struct AcpiContext {
    tables: BTreeMap<TypeId, Box<dyn Any>>
}

unsafe impl Sync for AcpiContext {}
unsafe impl Send for AcpiContext {}

impl AcpiContext {
    pub fn new(rsdp: usize) -> Self {
        let handler = MainTableParser;

        let acpi = unsafe { 
            AcpiTables::from_rsdp(handler, rsdp)
                .expect("ACPI parse failed")
        };

        let mut ctx = Self {
            tables: BTreeMap::new()
        };

        ctx.load_table::<MadTable>(&acpi);
        ctx.load_table::<HpetTableParsed>(&acpi);

        ctx
    }

    fn load_table<T: AcpiParsedTable>(
        &mut self,
        acpi: &AcpiTables<MainTableParser>,
    ) {
        if let Some(mapping) = acpi.find_table::<T::Raw>() {
            let parsed = T::parse(mapping);
            self.tables.insert(TypeId::of::<T>(), Box::new(parsed));
        }
    }

    pub fn get_table<T: 'static>(&self) -> Option<&T> {
        self.tables
            .get(&TypeId::of::<T>())
            .and_then(|t| t.downcast_ref::<T>())
    }

}

fn map_acpi_regions(memmap: &[&Entry]) {
    let flags =
        PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;

    for entry in memmap {
        if !matches!(entry.entry_type, EntryType::ACPI_RECLAIMABLE | EntryType::ACPI_NVS) {
            continue;
        }

        let base = entry.base as usize;
        let len  = entry.length as usize;

        let start = align_down(base, PAGE_SIZE);
        let end   = align_up(base + len, PAGE_SIZE);

        let mut p = start;
        while p < end {
            let va = unsafe { HHDM_OFFSET } as u64 + (p as u64);

            kmap_page(
                VirtAddr::new(va),
                PhysAddr::new(p as u64),
                flags,
            );

            p += PAGE_SIZE;
        }
    }
}

pub fn init_acpi(rsdp_address: usize, memmap: &[&Entry]) {
    map_acpi_regions(memmap);

    ACPI_CTX.call_once(|| {
        RwLock::new(AcpiContext::new(rsdp_address))
    });
}

#[inline]
pub fn get_acpi_tables() -> &'static RwLock<AcpiContext>{
    ACPI_CTX
        .get()
        .expect("ACPI not inited yet")
}