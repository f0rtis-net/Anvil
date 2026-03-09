use core::cell::UnsafeCell;

use x86_64::{PhysAddr, VirtAddr, registers::control::Cr3, structures::paging::{Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB}};

use crate::{arch::amd64::{gdt::{KERNEL_CODE_SELECTOR, KERNEL_DATA_SELECTOR, USER_CODE_SELECTOR, USER_DATA_SELECTOR}, memory::{misc::{pages_to_order, phys_to_virt}, pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order}, vmm::{KernelFrameAllocator, PAGE_SIZE, create_new_pt4_from_kernel_pt4, kernel_pt}}, scheduler::{elf::ElfParsed, stack::{DEFAULT_KERNEL_STACK_SIZE, allocate_kernel_stack}, task::{Task, TaskId, TaskIdIndex, TaskRegisters, TaskState}}}, early_println};

const RFLAGS_WITH_IR: u64 = 0x202;
const USER_STACK_PAGES_COUNT: usize = 4;
const USER_STACK_TOP_VIRT_ADDR: u64 = 0x7FFF_FFFF_0000;


fn phys_to_offset_page_table(table: PhysAddr) -> OffsetPageTable<'static> {
    let phys_offset = kernel_pt().lock().phys_offset();
    let virt = phys_offset + table.as_u64();
    let page_table_ptr = virt.as_mut_ptr::<PageTable>();
    unsafe { OffsetPageTable::new(&mut *page_table_ptr, phys_offset) }
}

pub fn make_user_task(bytes: &[u8], task_id: TaskIdIndex) -> Result<Task, &'static str> {
    let elf = ElfParsed::parse(bytes).ok_or("failed to parse ELF")?;

    let new_pml4_phys = create_new_pt4_from_kernel_pt4();
    let mut pt = phys_to_offset_page_table(new_pml4_phys);

    for segment in &elf.segments {
        let seg_data = &bytes[segment.file_offset as usize
            ..segment.file_offset as usize + segment.raw_header.p_filesz as usize];

        let page_start = segment.vaddr.align_down(4096u64);
        let page_end = (segment.vaddr + segment.mem_size).align_up(4096u64);
        let page_count = (page_end - page_start) / 4096;

        let pt_flags = segment.flags.page_table_entry_flags();

        for i in 0..page_count {
            let page_va = page_start + i * 4096;
            let page = Page::<Size4KiB>::containing_address(page_va);

            let phys = alloc_pages_by_order(0, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
                .expect("failed to alloc segment page");
            let frame = PhysFrame::<Size4KiB>::containing_address(phys);

            unsafe {
                pt.map_to(page, frame, pt_flags, &mut KernelFrameAllocator)
                    .unwrap()
                    .flush();
            }

            let page_phys_virt = phys_to_virt(phys.as_u64() as usize) as *mut u8;

            let page_va_start = page_va.as_u64();
            let page_va_end = page_va_start + 4096;

            let seg_va_start = segment.vaddr.as_u64();
            let seg_va_end = seg_va_start + segment.raw_header.p_filesz;

            let copy_start = seg_va_start.max(page_va_start);
            let copy_end = seg_va_end.min(page_va_end);

            if copy_start < copy_end {
                let dst_offset = (copy_start - page_va_start) as usize;
                let src_offset = (copy_start - seg_va_start) as usize;
                let len = (copy_end - copy_start) as usize;

                unsafe {
                    core::ptr::copy_nonoverlapping(
                        seg_data.as_ptr().add(src_offset),
                        page_phys_virt.add(dst_offset),
                        len,
                    );
                }
            }

        }
    }

    let order = pages_to_order(USER_STACK_PAGES_COUNT);

    let stack_bottom_phys = alloc_pages_by_order(order, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
        .expect("failed to allocate stack page");

    let stack_addr_top = USER_STACK_TOP_VIRT_ADDR;
    let stack_size = PAGE_SIZE * USER_STACK_PAGES_COUNT;
    let stack_addr_bottom = stack_addr_top - stack_size as u64;

    let stack_bottom_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_addr_bottom));
    let stack_top_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_addr_top - 1));

    let flags = PageTableFlags::PRESENT
                | PageTableFlags::USER_ACCESSIBLE
                | PageTableFlags::WRITABLE
                | PageTableFlags::NO_EXECUTE;

    let mut curr_phys = stack_bottom_phys.as_u64();
    
    for page in Page::range_inclusive(stack_bottom_page, stack_top_page) {
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(curr_phys));
        curr_phys += PAGE_SIZE as u64;

        unsafe {
            pt.map_to(page, frame, flags, &mut KernelFrameAllocator)
                .unwrap()
                .flush();
        }
    }
    
    let kernel_stack = allocate_kernel_stack(DEFAULT_KERNEL_STACK_SIZE);

    let registers = TaskRegisters {
        rflags: RFLAGS_WITH_IR,
        cs: USER_CODE_SELECTOR.0 as u64,
        ss: USER_DATA_SELECTOR.0 as u64,
        rsp: stack_addr_top - 8, 
        rip: elf.entrypoint.as_u64(),
        ..Default::default()
    };

    Ok(Task {
        id: TaskId::new(task_id),
        kernel_stack,
        registers: UnsafeCell::new(registers),
        page_table: new_pml4_phys,
        addr_space: None,
        task_state: TaskState::Ready,
    })
}

pub fn make_kernel_task(id: TaskId, entry_point: u64) -> Task {
    let phys_frame = Cr3::read().0;
    let phys_add_of_pt = phys_frame.start_address();
    let kernel_stack = allocate_kernel_stack(DEFAULT_KERNEL_STACK_SIZE);
    
    let registers = TaskRegisters {
        rflags: RFLAGS_WITH_IR,
        ss: KERNEL_DATA_SELECTOR.0 as u64,
        cs: KERNEL_CODE_SELECTOR.0 as u64 ,
        rsp: kernel_stack.top.as_u64(),
        rip: entry_point,
        ..Default::default()
    };

    Task {
        id,
        kernel_stack,
        registers: UnsafeCell::new(registers),
        page_table: phys_add_of_pt,
        addr_space: None,
        task_state: TaskState::Ready,
    }
}
