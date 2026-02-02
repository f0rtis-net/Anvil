use core::ptr::{self, NonNull};

use acpi::{Handler, PhysicalMapping};

use crate::arch::amd64::{cpu::hlt_loop, memory::misc::phys_to_virt, ports::Port};

#[derive(Clone)]
pub struct MainTableParser;

#[inline(always)]
fn vread<T: Copy>(addr: usize) -> T {
    unsafe { ptr::read_volatile(addr as *const T) }
}

#[inline(always)]
fn vwrite<T: Copy>(addr: usize, val: T) {
    unsafe { ptr::write_volatile(addr as *mut T, val) }
}

impl Handler for MainTableParser {
    unsafe fn map_physical_region<T>(&self, physical_address: usize, size: usize) -> PhysicalMapping<Self, T> {
        let va = phys_to_virt(physical_address);

        let virt_ptr = NonNull::new(va as *mut T)
            .expect("HHDM produced null virtual address for ACPI mapping");

        PhysicalMapping {
            physical_start: physical_address,
            virtual_start: virt_ptr,
            region_length: size,
            mapped_length: size,
            handler: self.clone(),
        }
    }

    fn unmap_physical_region<T>(_: &acpi::PhysicalMapping<Self, T>) {

    }

    fn read_u8(&self, address: usize) -> u8 {
        vread::<u8>(phys_to_virt(address))
    }

    fn read_u16(&self, address: usize) -> u16 {
        vread::<u16>(phys_to_virt(address))
    }

    fn read_u32(&self, address: usize) -> u32 {
        vread::<u32>(phys_to_virt(address))
    }

    fn read_u64(&self, address: usize) -> u64 {
        vread::<u64>(phys_to_virt(address))
    }

    fn write_u8(&self, address: usize, value: u8) {
        vwrite::<u8>(phys_to_virt(address), value)
    }

    fn write_u16(&self, address: usize, value: u16) {
        vwrite::<u16>(phys_to_virt(address), value)
    }

    fn write_u32(&self, address: usize, value: u32) {
        vwrite::<u32>(phys_to_virt(address), value)
    }

    fn write_u64(&self, address: usize, value: u64) {
        vwrite::<u64>(phys_to_virt(address), value)
    }

    fn read_io_u8(&self, port: u16) -> u8 {
        Port::<u8>::new(port).read() 
    }

    fn read_io_u16(&self, port: u16) -> u16 {
        Port::<u16>::new(port).read() 
    }

    fn read_io_u32(&self, port: u16) -> u32 {
        Port::<u32>::new(port).read() 
    }

    fn write_io_u8(&self, port: u16, value: u8) {
        Port::<u8>::new(port).write(value) 
    }

    fn write_io_u16(&self, port: u16, value: u16) {
        Port::<u16>::new(port).write(value) 
    }

    fn write_io_u32(&self, port: u16, value: u32) {
        Port::<u32>::new(port).write(value) 
    }

    fn read_pci_u8(&self, address: acpi::PciAddress, offset: u16) -> u8 {
        todo!()
    }

    fn read_pci_u16(&self, address: acpi::PciAddress, offset: u16) -> u16 {
        todo!()
    }

    fn read_pci_u32(&self, address: acpi::PciAddress, offset: u16) -> u32 {
        todo!()
    }

    fn write_pci_u8(&self, address: acpi::PciAddress, offset: u16, value: u8) {
        todo!()
    }

    fn write_pci_u16(&self, address: acpi::PciAddress, offset: u16, value: u16) {
        todo!()
    }

    fn write_pci_u32(&self, aaddress: acpi::PciAddress, offset: u16, value: u32) {
        todo!()
    }

    fn nanos_since_boot(&self) -> u64 {
        0
    }

    fn stall(&self, microseconds: u64) {
        hlt_loop()
    }

    fn sleep(&self, milliseconds: u64) {
        hlt_loop()
    }

    fn create_mutex(&self) -> acpi::Handle {
        acpi::Handle(0)
    }

    fn acquire(&self, mutex: acpi::Handle, timeout: u16) -> Result<(), acpi::aml::AmlError> {
        todo!()
    }

    fn release(&self, mutex: acpi::Handle) {}
}