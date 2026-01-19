use spin::{Mutex, Once};
use x86_64::{PhysAddr, VirtAddr, registers::control::Cr3, structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB}};

use crate::{arch::amd64::memory::{misc::phys_to_virt, pmm::{pages_allocator::{PAllocFlags, alloc_pages_by_order}}}};

static KERNEL_PT: Once<Mutex<OffsetPageTable>> = Once::new();

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

fn get_active_page_table4(hhdm_offset: u64) -> &'static mut PageTable {
    let (lvl4_table_frame, _) = Cr3::read();

    let phys = lvl4_table_frame.start_address();
    let virt = phys_to_virt(hhdm_offset as usize, phys.as_u64() as usize);

    let table_ptr: *mut PageTable = virt as *mut PageTable;

    unsafe { &mut *table_ptr }
}

pub fn init_virtual_memory(hhdm_offset: u64) {
    unsafe {
        let lvl4_table = get_active_page_table4(hhdm_offset);
        KERNEL_PT.call_once(|| {
            Mutex::new(OffsetPageTable::new(lvl4_table, VirtAddr::new(hhdm_offset)))
        });
    }
}

pub fn map_page(
    virt_addr: VirtAddr,
    phys_addr: PhysAddr,
    flags: PageTableFlags,
) {
    let page = Page::<Size4KiB>::containing_address(virt_addr);
    let frame = PhysFrame::<Size4KiB>::containing_address(phys_addr);

    let mut frame_alloc = KernelFrameAllocator;

    unsafe {
        kernel_pt().lock().map_to(page, frame, flags, &mut frame_alloc)
            .expect("map_page: map_to failed")
            .flush();
    }
}

pub fn unmap_page(virt_addr: VirtAddr) {
    let page = Page::<Size4KiB>::containing_address(virt_addr);

    let (_frame, flush) = kernel_pt().lock()
        .unmap(page)
        .expect("unmap_page: page not mapped");

    flush.flush();
}
