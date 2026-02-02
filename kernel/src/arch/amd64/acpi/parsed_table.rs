use acpi::{AcpiTable, PhysicalMapping};

use crate::arch::amd64::acpi::main_table_parser::MainTableParser;

pub trait AcpiParsedTable: 'static {
    type Raw: AcpiTable;
    fn parse(mapping: PhysicalMapping<MainTableParser, Self::Raw>) -> Self;
}