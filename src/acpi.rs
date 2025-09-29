use core::ptr::NonNull;

use acpi::{AcpiHandler, AcpiTables, PhysicalMapping};
use x86_64::VirtAddr;

use crate::println;


#[derive(Clone)]
pub struct KernelAcpiMapper {
    phys_offset: VirtAddr,
}

impl AcpiHandler for KernelAcpiMapper {
    unsafe fn map_physical_region<T>(
        &self,
        paddr: usize,
        size: usize,
    ) -> PhysicalMapping<Self, T> {
        let vaddr = self.phys_offset + (paddr as u64);
        let ptr = NonNull::new(vaddr.as_mut_ptr::<T>()).unwrap();
        unsafe { PhysicalMapping::new(
            paddr,
            ptr,
            size,
            size,
            self.clone(),
        ) }
    }

    fn unmap_physical_region<T>(_region: &PhysicalMapping<Self, T>) {
       
    }
}

pub fn init_acpi(phys_offset: VirtAddr) {
    let handler = KernelAcpiMapper { phys_offset };

    let tables = unsafe { AcpiTables::search_for_rsdp_bios(handler) 
        .expect("ACPI: RSDP not found") };

    let platform = tables.platform_info().expect("ACPI: platform info fail");

    println!("CPU info: {:?}", platform.processor_info);    
}