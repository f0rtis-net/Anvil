use bootloader::bootinfo::{MemoryMap, MemoryRegionType};
use x86_64::structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTableFlags, PhysFrame, Size4KiB};
use x86_64::PhysAddr;
use x86_64::{structures::paging::PageTable, VirtAddr};

pub struct BootInfoFrameAllocator {
    memory_map: &'static MemoryMap,
    next: usize
}

impl BootInfoFrameAllocator {
    pub unsafe fn init(memory_map: &'static MemoryMap) -> Self {
        BootInfoFrameAllocator {
            memory_map,
            next: 0,
        }
    }

    fn usable_frames(&self) -> impl Iterator<Item = PhysFrame> {
        let regions = self.memory_map.iter();
        let usable_regions = regions
            .filter(|r| r.region_type == MemoryRegionType::Usable);
        let addr_ranges = usable_regions
            .map(|r| r.range.start_addr()..r.range.end_addr());
        let frame_addresses = addr_ranges.flat_map(|r| r.step_by(4096));
        frame_addresses.map(|addr| PhysFrame::containing_address(PhysAddr::new(addr)))
    }
}

unsafe impl FrameAllocator<Size4KiB> for BootInfoFrameAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame> {
        let frame = self.usable_frames().nth(self.next);
        self.next += 1;
        frame
    }
}

unsafe fn active_level_4_table(physical_memory_offset: VirtAddr)
    -> &'static mut PageTable
{
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    unsafe { &mut *page_table_ptr }
}

pub unsafe fn init_vmemory(physical_memory_offset: VirtAddr) -> OffsetPageTable<'static> {
    unsafe {
        let level_4_table = active_level_4_table(physical_memory_offset);
        OffsetPageTable::new(level_4_table, physical_memory_offset)
    }
}

pub struct Vma {
    pub start_page: Page,
    pub stack_addr: u64,
    pub eip: u64,
    pub pages_allocated: usize
}

impl Vma {
    pub fn new(
        start_addr: u64,
        stack_size_in_pages: usize, 
        code_size_in_pages: usize,
        mapper: &mut impl Mapper<Size4KiB>,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>, 
    ) -> Vma {
        let start_addr = VirtAddr::new(start_addr);

        let start_page: Page<Size4KiB> = Page::containing_address(start_addr);
        let end_page: Page<Size4KiB> = start_page + (stack_size_in_pages + code_size_in_pages) as u64;

        let page_range = Page::range_inclusive(start_page, end_page);

        for page in page_range {
            let phys_frame = frame_allocator.allocate_frame().unwrap();

            let flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;

            unsafe {
                mapper.map_to(page, phys_frame, flags, frame_allocator).unwrap().flush()
            };
        }

        let stack_page = start_page + (code_size_in_pages as u64);
        let stack_addr = stack_page.start_address().as_u64();

        Vma {
            start_page,
            stack_addr,
            eip: start_addr.as_u64(),
            pages_allocated: page_range.count()
        }
    }
}