use core::ptr;

use alloc::vec::Vec;
use x86_64::VirtAddr;

use crate::{arch::amd64::{acpi::{get_acpi_tables, madt::MadTable}, memory::{misc::phys_to_virt, pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order}, vmm::PAGE_SIZE}}, define_per_cpu_u32};

unsafe extern "C" {
    static _percpu_start: u8;
    static _percpu_data_end: u8;
    static _percpu_end: u8;
    static _percpu_load: u8;
    static _percpu_vma_base: u8;
}

pub struct PerCpuRegion {
    pub base: VirtAddr,
}

pub struct PerCpuTemplate {
    pub data_size: usize,
    pub bss_size: usize,
    pub total_size: usize,
    pub load_ptr: *const u8, 
}

#[inline(always)]
fn addr(sym: *const u8) -> usize {
    sym as usize
}

#[inline(always)]
fn align_up_usize(x: usize, align: usize) -> usize {
    (x + align - 1) & !(align - 1)
}

const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102;

#[inline(always)]
fn wrmsr(msr: u32, val: u64) {
    unsafe {
        let lo = val as u32;
        let hi = (val >> 32) as u32;
        core::arch::asm!(
            "wrmsr",
            in("ecx") msr,
            in("eax") lo,
            in("edx") hi,
            options(nostack, preserves_flags),
        );
    }
}


#[inline(always)]
pub fn set_gsbase_for_percpu_region(region_base: VirtAddr) {
    let percpu_vma_base = core::ptr::addr_of!(_percpu_vma_base) as u64;
    let kgs_delta = region_base.as_u64().wrapping_sub(percpu_vma_base);

    let hi = kgs_delta >> 48;
    if hi != 0 && hi != 0xFFFF {
        panic!("non-canonical GS base computed: {:#x}", kgs_delta);
    }
    
    wrmsr(MSR_KERNEL_GS_BASE, kgs_delta);
}

fn percpu_template() -> PerCpuTemplate {
    let percpu_start    = ptr::addr_of!(_percpu_start) as *const u8;
    let percpu_data_end = ptr::addr_of!(_percpu_data_end) as *const u8;
    let percpu_end      = ptr::addr_of!(_percpu_end) as *const u8;

    let data_size = addr(percpu_data_end) - addr(percpu_start);
    let bss_size  = addr(percpu_end) - addr(percpu_data_end);

    let total_size = align_up_usize(data_size + bss_size, PAGE_SIZE as usize);

    let load_ptr = ptr::addr_of!(_percpu_load) as *const u8;

    PerCpuTemplate { data_size, bss_size, total_size, load_ptr }
}

fn construct_region_from_template(dst: *mut u8, tpl: &PerCpuTemplate) {
    unsafe {
        // .percpu.data
        ptr::copy_nonoverlapping(tpl.load_ptr, dst, tpl.data_size);
        // .percpu.bss
        ptr::write_bytes(dst.add(tpl.data_size), 0, tpl.bss_size);
    }
}

fn alloc_percpu_region(total_size: usize) -> VirtAddr {
    let pages = (total_size + PAGE_SIZE as usize - 1) / PAGE_SIZE as usize;

    if pages == 1 {
        let phys = alloc_pages_by_order(0, PAllocFlags::Kernel | PAllocFlags::Zeroed)
            .expect("percpu page alloc failed");
        return VirtAddr::new(phys_to_virt(phys.as_u64() as usize) as u64);
    }

    let order = (pages.next_power_of_two().trailing_zeros()) as usize;
    let phys = alloc_pages_by_order(order, PAllocFlags::Kernel | PAllocFlags::Zeroed)
        .expect("percpu pages alloc failed");

    VirtAddr::new(phys_to_virt(phys.as_u64() as usize) as u64)
}

pub fn init_percpu_regions() -> Vec<PerCpuRegion> {
    let cpu_count = get_acpi_tables().read().get_table::<MadTable>().expect("Unable to get cpu_count info form madt").cpus.len();

    let tpl = percpu_template();

    let mut regions = Vec::with_capacity(cpu_count);

    for _ in 0..cpu_count {
        let base = alloc_percpu_region(tpl.total_size);

        construct_region_from_template(base.as_mut_ptr(), &tpl);

        regions.push(PerCpuRegion {
            base,
        });
    }

    regions
}

define_per_cpu_u32!(CPU_ID);

pub fn set_cpu_id(id: u32) {
    set_per_cpu_CPU_ID(id);
}

pub fn get_cpu_id_no_guard() -> u32 {
    return get_per_cpu_no_guard_CPU_ID();
}