use core::{arch::naked_asm, cell::UnsafeCell, sync::atomic::{AtomicU8, AtomicU64}};

use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, instructions::interrupts, registers::control::Cr3, structures::paging::{Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB}};

use crate::arch::amd64::{ipc::cnode::CNode, memory::{misc::{pages_to_order, phys_to_virt}, pmm::{HHDM_OFFSET, pages_allocator::{PAllocFlags, alloc_pages_by_order}}, vmm::{KernelFrameAllocator, PAGE_SIZE, create_new_pt4_from_kernel_pt4, kernel_pt}}, scheduler::{addr_space::AddrSpace, stack::{DEFAULT_KERNEL_STACK_SIZE, allocate_kernel_stack}, task::{Task, TaskId, TaskIdIndex, TaskRegisters, TaskState}}};

const RFLAGS_WITH_IR: u64 = 0x202;
const USER_STACK_PAGES_COUNT: usize = 4;
const USER_STACK_TOP_VIRT_ADDR: u64 = 0x7FFF_FFFF_0000;

pub const USER_LOAD_VADDR: u64 = 0x400000;
pub const USER_ENTRY_VADDR: u64 = USER_LOAD_VADDR; 
pub const BOOTINFO_VADDR: u64 = 0x1000;

fn phys_to_offset_page_table(table: PhysAddr) -> OffsetPageTable<'static> {
    let phys_offset = kernel_pt().lock().phys_offset();
    let virt = phys_offset + table.as_u64();
    let page_table_ptr = virt.as_mut_ptr::<PageTable>();
    unsafe { OffsetPageTable::new(&mut *page_table_ptr, phys_offset) }
}

#[repr(C)]
pub struct InitSvrsBootInfo {
    pub self_tcb_cap:    u64,
    pub self_vspace_cap: u64,
    pub self_cnode_cap:  u64,
}

pub fn make_init_task(
    bytes: &[u8],
    task_id: TaskIdIndex,
    bootinfo: InitSvrsBootInfo,
) -> Result<Task, &'static str> {
    let new_pml4_phys = create_new_pt4_from_kernel_pt4();
    let mut pt = phys_to_offset_page_table(new_pml4_phys);

    let page_count = bytes.len().div_ceil(PAGE_SIZE);
    for i in 0..page_count {
        let va = VirtAddr::new(USER_LOAD_VADDR + (i * PAGE_SIZE) as u64);
        let page = Page::<Size4KiB>::containing_address(va);

        let phys = alloc_pages_by_order(0, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
            .expect("make_init_task: OOM");
        let frame = PhysFrame::<Size4KiB>::containing_address(phys);

        let src_offset = i * PAGE_SIZE;
        let src_end = (src_offset + PAGE_SIZE).min(bytes.len());
        let copy_len = src_end - src_offset;

        unsafe {
            let dst = phys_to_virt(phys.as_u64() as usize) as *mut u8;
            core::ptr::copy_nonoverlapping(
                bytes.as_ptr().add(src_offset),
                dst,
                copy_len,
            );

            pt.map_to(
                page,
                frame,
                PageTableFlags::PRESENT
                    | PageTableFlags::USER_ACCESSIBLE
                    | PageTableFlags::WRITABLE,
                &mut KernelFrameAllocator,
            )
            .unwrap()
            .flush();
        }
    }

    let order = pages_to_order(USER_STACK_PAGES_COUNT);
    let stack_bottom_phys = alloc_pages_by_order(order, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
        .expect("make_init_task: stack OOM");

    let stack_size   = PAGE_SIZE * USER_STACK_PAGES_COUNT;
    let stack_top_va = USER_STACK_TOP_VIRT_ADDR;
    let stack_bot_va = stack_top_va - stack_size as u64;

    let stack_bot_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_bot_va));
    let stack_top_page = Page::<Size4KiB>::containing_address(VirtAddr::new(stack_top_va - 1));

    let stack_flags = PageTableFlags::PRESENT
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::WRITABLE
        | PageTableFlags::NO_EXECUTE;

    let mut curr_phys = stack_bottom_phys.as_u64();
    for page in Page::range_inclusive(stack_bot_page, stack_top_page) {
        let frame = PhysFrame::<Size4KiB>::containing_address(PhysAddr::new(curr_phys));
        curr_phys += PAGE_SIZE as u64;
        unsafe {
            pt.map_to(page, frame, stack_flags, &mut KernelFrameAllocator)
                .unwrap()
                .flush();
        }
    }

    let bootinfo_phys = alloc_pages_by_order(0, PAllocFlags::KERNEL | PAllocFlags::ZEROED)
        .expect("make_init_task: bootinfo OOM");

    unsafe {
        let dst = phys_to_virt(bootinfo_phys.as_u64() as usize) as *mut InitSvrsBootInfo;
        core::ptr::write(dst, bootinfo);
    }

    let bootinfo_page = Page::<Size4KiB>::containing_address(VirtAddr::new(BOOTINFO_VADDR));
    let bootinfo_frame = PhysFrame::<Size4KiB>::containing_address(bootinfo_phys);

    unsafe {
        pt.map_to(
            bootinfo_page,
            bootinfo_frame,
            PageTableFlags::PRESENT
                | PageTableFlags::USER_ACCESSIBLE,
            &mut KernelFrameAllocator,
        )
        .unwrap()
        .flush();
    }

    // kernel stack + trampoline
    let kernel_stack = allocate_kernel_stack(DEFAULT_KERNEL_STACK_SIZE);
    let stack_top_ptr = kernel_stack.top.as_u64() as *mut u64;

    unsafe {
        stack_top_ptr.sub(1).write(stack_top_va - 8);           // rsp
        stack_top_ptr.sub(2).write(USER_ENTRY_VADDR);           // rip
        stack_top_ptr.sub(3).write(user_task_trampoline as u64);// ret
        for i in 4..=18 {
            stack_top_ptr.sub(i).write(0);
        }
        stack_top_ptr.sub(10).write(BOOTINFO_VADDR);            // rdi
    }

    let initial_rsp = unsafe { stack_top_ptr.sub(18) } as u64;

    Ok(Task {
        id: TaskId::new(task_id),
        kernel_stack,
        registers: UnsafeCell::new(TaskRegisters {
            rsp: initial_rsp,
            rdi: BOOTINFO_VADDR,
            ..Default::default()
        }),
        addr_space:   Mutex::new(AddrSpace::new(pt)),
        task_state:   AtomicU8::new(TaskState::Ready as u8),
        wake_at_tick: Mutex::new(AtomicU64::new(0)),
        cnode:        Mutex::new(CNode::new()),
    })
}

#[unsafe(naked)]
unsafe extern "C" fn user_task_trampoline() {
    naked_asm!(
        "sti",
        "pop rcx",
        "pop rsp",
        "mov r11, {rflags}",
        "swapgs",
        "sysretq",
        rflags = const RFLAGS_WITH_IR,
    );
}

extern "C" fn kernel_task_trampoline(entry: u64) -> ! {
    interrupts::enable();
    let func: extern "C" fn() -> ! = unsafe { core::mem::transmute(entry) };
    func();
}

pub fn make_kernel_task(id: TaskId, entry_point: u64) -> Task {
    let (phys_frame, _) = Cr3::read();
    let phys_addr_of_pt = phys_frame.start_address();
    
    let hhdm_offset = VirtAddr::new(unsafe { HHDM_OFFSET as u64 });
    let page_table = unsafe {
        let virt = phys_to_virt(phys_addr_of_pt.as_u64() as usize);
        let pml4 = &mut *(virt as *mut PageTable);
        OffsetPageTable::new(pml4, hhdm_offset)
    };

    let kernel_stack = allocate_kernel_stack(DEFAULT_KERNEL_STACK_SIZE);
    let stack_top_ptr = kernel_stack.top.as_u64() as *mut u64;
    unsafe {
        stack_top_ptr.sub(1).write(kernel_task_trampoline as u64);
        for i in 2..=16 {
            stack_top_ptr.sub(i).write(0);
        }
        stack_top_ptr.sub(8).write(entry_point);
    }
    let initial_rsp = unsafe { stack_top_ptr.sub(16) } as u64;

    Task {
        id,
        kernel_stack,
        registers: UnsafeCell::new(TaskRegisters {
            rsp: initial_rsp,
            ..TaskRegisters::default()
        }),
        addr_space: Mutex::new(AddrSpace::new(page_table)),
        task_state: AtomicU8::new(TaskState::Ready as u8),
        wake_at_tick: Mutex::new(AtomicU64::new(0)),
        cnode: Mutex::new(CNode::new())
    }
}
