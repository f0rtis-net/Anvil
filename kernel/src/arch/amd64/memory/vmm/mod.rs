use core::ptr;

use alloc::vec::Vec;
use spin::{Mutex, Once};
use x86_64::{PhysAddr, VirtAddr, registers::control::Cr3, structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageSize, PageTable, PageTableFlags, PhysFrame, Size4KiB, frame, mapper::MapperFlush, page::PageRangeInclusive}};

use crate::{arch::amd64::memory::{misc::phys_to_virt, pmm::{pages_allocator::{PAllocFlags, alloc_pages_by_order}}}};

mod pf_handler;
pub mod v_allocator;

static KERNEL_PT: Once<Mutex<OffsetPageTable>> = Once::new();

pub const PAGE_SIZE: usize = Size4KiB::SIZE as usize;

#[inline]
pub fn kernel_pt() -> &'static Mutex<OffsetPageTable<'static>>{
    return KERNEL_PT.get().expect("Jernel page table not initalized!")
}

pub struct KernelFrameAllocator;

unsafe impl FrameAllocator<Size4KiB> for KernelFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let phys = alloc_pages_by_order(0, PAllocFlags::Kernel | PAllocFlags::Zeroed)?;
        Some(PhysFrame::from_start_address(phys).unwrap())
    }
}

fn get_active_page_table4() -> &'static mut PageTable {
    let (lvl4_table_frame, _) = Cr3::read();

    let phys = lvl4_table_frame.start_address();
    let virt = phys_to_virt(phys.as_u64() as usize);

    let table_ptr: *mut PageTable = virt as *mut PageTable;

    unsafe { &mut *table_ptr }
}

pub fn init_virtual_memory(hhdm_offset: u64) {
    unsafe {
        let lvl4_table = get_active_page_table4();
        KERNEL_PT.call_once(|| {
            Mutex::new(OffsetPageTable::new(lvl4_table, VirtAddr::new(hhdm_offset)))
        });
    }
}

pub fn map_mmio_region(
    phys: PhysAddr,
    size: usize,
) -> VirtAddr {
    let phys_start = phys.as_u64() & !(PAGE_SIZE as u64 - 1);
    let offset     = phys.as_u64() - phys_start;
    let pages      = (offset as usize + size + 4095) / 4096;

    //TODO - VMALLOC
    let virt_base = VirtAddr::new(0xFFFF_FF80_0000_0000); 

    for i in 0..pages {
        let va = virt_base + i as u64 * PAGE_SIZE as u64;
        let pa = PhysAddr::new(phys_start + i as u64 * PAGE_SIZE as u64);

        kmap_page(
            va,
            pa,
            PageTableFlags::PRESENT
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_CACHE
                | PageTableFlags::NO_EXECUTE,
        );
    }

    virt_base + offset
}

pub fn kmap_page(
    virt_addr: VirtAddr,
    phys_addr: PhysAddr,
    flags: PageTableFlags,
) {
    let mut kernel_pt = kernel_pt().lock();
    map_with_custom_table(&mut kernel_pt, virt_addr, phys_addr, flags);
}

pub fn kunmap_page(virt_addr: VirtAddr) {
    let mut kernel_pt = kernel_pt().lock();
    unmap_page_with_custom_table(&mut kernel_pt, virt_addr);
}

pub fn map_with_custom_table(
    table: &mut OffsetPageTable,
    virt_addr: VirtAddr,
    phys_addr: PhysAddr,
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let page: Page<Size4KiB> =
        Page::containing_address(virt_addr);

    let frame: PhysFrame<Size4KiB> =
        PhysFrame::containing_address(phys_addr);

    let mut frame_alloc = KernelFrameAllocator;

    unsafe {
        let flush: MapperFlush<Size4KiB> = table
            .map_to(page, frame, flags, &mut frame_alloc)
            .map_err(|_| "map_to failed")?;

        flush.flush();
    }

    Ok(())
}

pub fn unmap_page_with_custom_table(
    table: &mut OffsetPageTable, 
    virt_addr: VirtAddr
) {
    let page = Page::<Size4KiB>::containing_address(virt_addr);

    let (_frame, flush) = table
        .unmap(page)
        .expect("unmap_page: page not mapped");

    flush.flush();
}

pub fn map_region_with_table(
    table: &mut OffsetPageTable,
    virtual_region: PageRangeInclusive<Size4KiB>,
    physical_region: &[PhysAddr],   
    flags: PageTableFlags,
) -> Result<(), &'static str> {
    let page_count = virtual_region.len() as usize;

    if physical_region.len() < page_count {
        return Err("physical_region is smaller than virtual_region");
    }

    let mut frame_alloc = KernelFrameAllocator;

    for (i, page) in virtual_region.into_iter().enumerate() {
        let phys = physical_region[i];

        let frame = PhysFrame::<Size4KiB>::containing_address(phys);

        unsafe {
            let flush: MapperFlush<Size4KiB> = table
                .map_to(page, frame, flags, &mut frame_alloc)
                .map_err(|_| "map_to failed")?;

            flush.flush();
        }
    }

    Ok(())
}

pub fn unmap_region_with_table(
    table: &mut OffsetPageTable,
    virtual_region: PageRangeInclusive<Size4KiB>
) -> Vec<PhysAddr> {
    let mut frames = Vec::<PhysAddr>::new();

    for page in virtual_region {
        let (_frame, flush) = kernel_pt().lock()
        .unmap(page)
        .expect("unmap_page: page not mapped");

        frames.push(_frame.start_address());

        flush.flush();
    }

    frames
}

pub fn create_new_pt4_from_kernel_pt4() -> PhysAddr {
    let new_pml4_phys = alloc_pages_by_order(
        0,
        PAllocFlags::Kernel | PAllocFlags::Zeroed,
    ).expect("create_new_addr_space: failed to alloc pml4");

    let new_pml4_virt =
        phys_to_virt(new_pml4_phys.as_u64() as usize) as *mut PageTable;

    let current_pml4 = get_active_page_table4() as *const PageTable;

    unsafe {
        let src = current_pml4 as *const u64;
        let dst = new_pml4_virt as *mut u64;

        ptr::copy_nonoverlapping(src.add(256), dst.add(256), 256);
    }

    new_pml4_phys
}
