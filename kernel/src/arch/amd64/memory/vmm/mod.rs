#![allow(dead_code)]

use core::ptr;

use alloc::vec::Vec;
use spin::{Mutex, Once};
use x86_64::{
    PhysAddr, VirtAddr,
    registers::control::Cr3,
    structures::paging::{
        FrameAllocator, Mapper, OffsetPageTable,
        Page, PageSize, PageTable, PageTableFlags,
        PhysFrame, Size4KiB,
        mapper::{MapToError},
        page::PageRangeInclusive,
    },
};

use crate::arch::amd64::memory::{
    misc::phys_to_virt,
    pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order},
};

mod pf_handler;
pub mod v_allocator;

pub const PAGE_SIZE: usize = Size4KiB::SIZE as usize;

static KERNEL_PT: Once<Mutex<OffsetPageTable<'static>>> = Once::new();

#[inline]
pub fn kernel_pt() -> &'static Mutex<OffsetPageTable<'static>> {
    KERNEL_PT.get().expect("Kernel page table not initialized")
}

pub struct KernelFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for KernelFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let phys = alloc_pages_by_order(0, PAllocFlags::KERNEL | PAllocFlags::ZEROED)?;
        Some(PhysFrame::from_start_address(phys).expect("PMM returned unaligned frame"))
    }
}

unsafe fn get_active_pml4() -> &'static mut PageTable {
    let (frame, _) = Cr3::read();
    let virt = phys_to_virt(frame.start_address().as_u64() as usize);
    unsafe { &mut *(virt as *mut PageTable) }
}

pub fn init_virtual_memory(hhdm_offset: u64) {
    KERNEL_PT.call_once(|| {
        let lvl4 = unsafe { get_active_pml4() };
        unsafe { Mutex::new(OffsetPageTable::new(lvl4, VirtAddr::new(hhdm_offset))) }
    });
}

pub fn map_mmio_region(phys: PhysAddr, size: usize) -> VirtAddr {
    let phys_start = phys.as_u64() & !(PAGE_SIZE as u64 - 1);
    let offset     = (phys.as_u64() - phys_start) as usize;
    let pages      = (offset + size).div_ceil(PAGE_SIZE);
    let virt_base  = VirtAddr::new(0xFFFF_FF80_0000_0000);
    let flags      = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_CACHE
        | PageTableFlags::NO_EXECUTE;

    let mut pt = kernel_pt().lock();
    for i in 0..pages {
        let va = virt_base + (i * PAGE_SIZE) as u64;
        let pa = PhysAddr::new(phys_start + (i * PAGE_SIZE) as u64);
        map_mmio_page_inner(&mut pt, va, pa, flags)
            .expect("map_mmio_region: failed to map page");
    }
    virt_base + offset as u64
}

pub fn kmap_page(virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags) {
    let mut pt = kernel_pt().lock();
    map_single_page(&mut pt, virt, phys, flags)
        .expect("kmap_page: map_to failed");
}

pub fn kmap_mmio_page(virt: VirtAddr, phys: PhysAddr, flags: PageTableFlags) {
    let mut pt = kernel_pt().lock();
    map_mmio_page_inner(&mut pt, virt, phys, flags)
        .expect("kmap_mmio_page: failed");
}

pub fn kunmap_page(virt: VirtAddr) {
    let mut pt = kernel_pt().lock();
    unmap_single_page(&mut pt, virt)
        .expect("kunmap_page: page was not mapped");
}

pub fn map_single_page(
    table: &mut OffsetPageTable,
    virt:  VirtAddr,
    phys:  PhysAddr,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let page  = Page::<Size4KiB>::containing_address(virt);
    let frame = PhysFrame::<Size4KiB>::containing_address(phys);
    let mut fa = KernelFrameAllocator;
    unsafe {
        table
            .map_to(page, frame, flags, &mut fa)
            .map_err(map_error_str)?
            .flush();
    }
    Ok(())
}

pub fn map_mmio_page_inner(
    table: &mut OffsetPageTable,
    virt:  VirtAddr,
    phys:  PhysAddr,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let page  = Page::<Size4KiB>::containing_address(virt);
    let frame = PhysFrame::<Size4KiB>::containing_address(phys);
    let mut fa = KernelFrameAllocator;

    match unsafe { table.map_to(page, frame, flags, &mut fa) } {
        Ok(flush) => {
            flush.flush();
            Ok(())
        }
        Err(MapToError::PageAlreadyMapped(existing)) => {
            if existing == frame {
                Ok(()) 
            } else {
                Err("map_mmio_page: VA already mapped to different PA")
            }
        }
        Err(MapToError::ParentEntryHugePage) => {
            Ok(()) 
        }
        Err(MapToError::FrameAllocationFailed) => {
            Err("map_mmio_page: OOM for intermediate page table")
        }
    }
}

pub fn unmap_single_page(
    table: &mut OffsetPageTable,
    virt:  VirtAddr,
) -> Result<PhysAddr, &'static str> {
    let page = Page::<Size4KiB>::containing_address(virt);
    let (frame, flush) = table.unmap(page).map_err(unmap_error_str)?;
    flush.flush();
    Ok(frame.start_address())
}

pub fn map_region(
    table:           &mut OffsetPageTable,
    virtual_region:  PageRangeInclusive<Size4KiB>,
    physical_region: &[PhysAddr],
    flags:           PageTableFlags,
) -> Result<(), &'static str> {
    let page_count = virtual_region.clone().count();
    if physical_region.len() < page_count {
        return Err("map_region: physical_region shorter than virtual_region");
    }
    for (i, page) in virtual_region.into_iter().enumerate() {
        let frame = PhysFrame::<Size4KiB>::containing_address(physical_region[i]);
        let mut fa = KernelFrameAllocator;
        unsafe {
            table
                .map_to(page, frame, flags, &mut fa)
                .map_err(map_error_str)?
                .flush();
        }
    }
    Ok(())
}

pub fn unmap_region(
    table:          &mut OffsetPageTable,
    virtual_region: PageRangeInclusive<Size4KiB>,
) -> Result<Vec<PhysAddr>, &'static str> {
    let mut frames = Vec::with_capacity(virtual_region.clone().count());
    for page in virtual_region {
        let (frame, flush) = table.unmap(page).map_err(unmap_error_str)?;
        flush.flush();
        frames.push(frame.start_address());
    }
    Ok(frames)
}

pub fn create_new_pt4_from_kernel_pt4() -> PhysAddr {
    let new_phys = alloc_pages_by_order(0, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
        .expect("create_new_pt4: OOM");
    let new_virt = phys_to_virt(new_phys.as_u64() as usize) as *mut PageTable;
    let cur_virt = unsafe { get_active_pml4() } as *const PageTable;
    unsafe {
        ptr::copy_nonoverlapping(
            (cur_virt as *const u64).add(256),
            (new_virt as *mut u64).add(256),
            256,
        );
    }
    new_phys
}

fn map_error_str(e: MapToError<Size4KiB>) -> &'static str {
    match e {
        MapToError::FrameAllocationFailed => "map: frame allocation failed",
        MapToError::ParentEntryHugePage   => "map: parent entry is a huge page",
        MapToError::PageAlreadyMapped(_)  => "map: page already mapped",
    }
}

fn unmap_error_str(e: x86_64::structures::paging::mapper::UnmapError) -> &'static str {
    use x86_64::structures::paging::mapper::UnmapError::*;
    match e {
        ParentEntryHugePage    => "unmap: parent entry is a huge page",
        PageNotMapped          => "unmap: page not mapped",
        InvalidFrameAddress(_) => "unmap: invalid frame address",
    }
}