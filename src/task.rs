use core::{pin::Pin};

use alloc::{boxed::Box, vec::Vec};
use alloc::vec;
use x86_64::structures::paging::PageSize;
use x86_64::{registers::control::Cr3, structures::paging::{FrameAllocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags, PhysFrame, Size4KiB, Translate}, VirtAddr};

use crate::{gdt::{kcs_sel, kds_sel, ucs_sel, uds_sel}, memory::vmem::{active_level_4_table, KERNEL_PT}, println};

struct ProgramHeader {
    p_type: u32,
    p_flags: u32,
    p_offset: u64,
    p_vaddr: VirtAddr,
    p_filesz: u64,
    p_memsz: u64,
    p_align: u64,
}

pub struct Elf {
    data: Pin<Box<[u8]>>,
    entry_point: VirtAddr,
    headers: Vec<ProgramHeader>,
}

impl Elf {
    pub fn new(data: Vec<u8>) -> Self {
        let data = Pin::new(data.into_boxed_slice());
        let s = &*data;

        if s.len() < 64 || &s[0..4] != b"\x7FELF" {
            panic!("Not an ELF file or too small");
        }

        if s[4] != 2 {
            panic!("Not ELF64");
        }
        if s[5] != 1 {
            panic!("Not little-endian ELF");
        }

        // e_entry @ offset 24, 8 bytes
        let e_entry = u64::from_le_bytes(s[24..32].try_into().unwrap());
        // e_phoff @ 32, 8 bytes
        let e_phoff = u64::from_le_bytes(s[32..40].try_into().unwrap()) as usize;
        // e_phentsize @ 54 (2 bytes)
        let e_phentsize = u16::from_le_bytes(s[54..56].try_into().unwrap()) as usize;
        // e_phnum @ 56 (2 bytes)
        let e_phnum = u16::from_le_bytes(s[56..58].try_into().unwrap()) as usize;

        let mut headers = Vec::new();
        for i in 0..e_phnum {
            let off = e_phoff + i * e_phentsize;
            if off + e_phentsize > s.len() {
                panic!("Program header out of bounds");
            }
            let ph = &s[off..off + e_phentsize];

            // ELF64 Program header layout:
            // p_type (4), p_flags (4), p_offset (8), p_vaddr (8), p_paddr (8),
            // p_filesz (8), p_memsz (8), p_align (8)
            let p_type = u32::from_le_bytes(ph[0..4].try_into().unwrap());
            let p_flags = u32::from_le_bytes(ph[4..8].try_into().unwrap());
            let p_offset = u64::from_le_bytes(ph[8..16].try_into().unwrap());
            let p_vaddr = VirtAddr::new(u64::from_le_bytes(ph[16..24].try_into().unwrap()));
            // p_paddr skipped (24..32)
            let p_filesz = u64::from_le_bytes(ph[32..40].try_into().unwrap());
            let p_memsz = u64::from_le_bytes(ph[40..48].try_into().unwrap());
            let p_align = u64::from_le_bytes(ph[48..56].try_into().unwrap());

            headers.push(ProgramHeader {
                p_type,
                p_flags,
                p_offset,
                p_vaddr,
                p_filesz,
                p_memsz,
                p_align,
            });
        }

        Self {
            data,
            entry_point: VirtAddr::new(e_entry),
            headers,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskState {
    Running,
    Ready,
    Zombie
}   

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaskPriority{
    Low,
    Normal,
    High,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Copy)]
pub struct Context {
    pub rbp: u64,
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

pub struct Task {
    pub pid: usize,
    pub ctx: Context,
    pub state: TaskState,
    pub quant: usize,
    pub exit_code: i64,
    pub ticks_left: usize,
    pub pt_phys: PhysFrame,
    pub kernel_stack: Box<[u8]>,
}

const STACK_SIZE: usize = 4096;

pub struct TaskManager {
    curr_pid: usize
}

impl TaskManager {
    pub fn new() -> Self {
        Self { 
            curr_pid: 0
        }
    }

    fn allocate_pages(
        &self,
        start_addr: u64,
        pages_ammo: usize,
        flags: PageTableFlags,
        mapper: &mut impl Mapper<Size4KiB>,
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
    ) -> (u64, u64) {
        assert!(pages_ammo >= 1, "Need 1 & more pages for map");

        let first_page: Page<Size4KiB> =
            Page::containing_address(VirtAddr::new(start_addr));

        let last_page = first_page + (pages_ammo - 1) as u64;

        for page in Page::range_inclusive(first_page, last_page) {
            let frame = frame_allocator
                .allocate_frame()
                .expect("Memory allocation limit");
            unsafe { mapper.map_to(page, frame, flags, frame_allocator).unwrap().flush() };
        }

        (first_page.start_address().as_u64(), last_page.start_address().as_u64())
    }

    fn fill_main_registers(&self, ctx: &mut Context) {
        ctx.r15 = 0;
        ctx.r14 = 0;
        ctx.r13 = 0;
        ctx.r12 = 0;
        ctx.r11 = 0;
        ctx.r10 = 0;
        ctx.r9  = 0;
        ctx.r8  = 0;
        ctx.rsi = 0;
        ctx.rdi = 0;
        ctx.rbp = 0;
        ctx.rbx = 0;
        ctx.rdx = 0;
        ctx.rcx = 0;
        ctx.rax = 0;

        //set flag to ready for interrupting
        ctx.rflags = 1 << 9;  
    }

    fn select_quants(&self, priority: &TaskPriority) -> usize {
        match priority {
            TaskPriority::High => 10,
            TaskPriority::Normal => 5,
            TaskPriority::Low => 1
        }
    }

    fn get_pid(&mut self) -> usize {
        let result = self.curr_pid;
        self.curr_pid += 1;
        result
    }

    unsafe fn pick_free_user_slot(&self, pml4_kernel: &PageTable) -> usize {
        for i in 0..256 {
            if pml4_kernel[i].is_unused() {
                return i;
            }
        }
        panic!("No free PML4 slot in lower half for user space");
    }   

    pub unsafe fn new_user<A: FrameAllocator<Size4KiB>>(
        &self,
        fa: &mut A,
        phys_offset: VirtAddr,
        kernel_pml4: &PageTable,
    ) -> (&'static mut PageTable, PhysFrame, usize) {
        let pml4_frame = fa.allocate_frame().expect("no memory for new PML4");
        let pml4_virt = (pml4_frame.start_address().as_u64() + phys_offset.as_u64()) as *mut PageTable;
        let pml4: &mut PageTable = &mut *pml4_virt;
        pml4.zero();

        for (i, e) in kernel_pml4.iter().enumerate() {
            if !e.is_unused() && !e.flags().contains(PageTableFlags::USER_ACCESSIBLE) {
                pml4[i] = e.clone();
            }
        }

        for i in 0..512 {
            if !pml4[i].is_unused() {
                if let Ok(f) = kernel_pml4[i].frame() {
                    if f == Cr3::read().0 {
                        let flags = pml4[i].flags();
                        pml4[i].set_frame(
                            pml4_frame,
                            flags, 
                        );
                    }
                }
            }
        }

        let user_slot = self.pick_free_user_slot(kernel_pml4);

        let user_pdpt = fa.allocate_frame().expect("no memory for user PDPT");
        let user_pdpt_virt = (user_pdpt.start_address().as_u64() + phys_offset.as_u64()) as *mut PageTable;
        let user_pdpt_tbl: &mut PageTable = &mut *user_pdpt_virt;
        user_pdpt_tbl.zero();

        pml4[user_slot].set_frame(
            user_pdpt,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE,
        );

        (pml4, pml4_frame, user_slot)
    }

    pub fn create_user_task(
        &mut self, priority: TaskPriority, 
        bin: &[u8], 
        frame_allocator: &mut impl FrameAllocator<Size4KiB>,
        phys_offset: VirtAddr
    ) -> Task {
        let mut ctx = Context::default();
        self.fill_main_registers(&mut ctx);

        let elf = Elf::new(bin.to_vec());

        let mut new_pt = unsafe { 
            self.new_user(frame_allocator, phys_offset, active_level_4_table(phys_offset)) 
        };

        let mut user_mapper = unsafe { 
            OffsetPageTable::new(&mut new_pt.0, phys_offset) 
        };

        for ph in &elf.headers {
            const PT_LOAD: u32 = 1;
            if ph.p_type != PT_LOAD {
                continue;
            }

            let seg_va = ph.p_vaddr.as_u64();
            let file_off = ph.p_offset as usize;
            let filesz = ph.p_filesz as usize;
            let memsz = ph.p_memsz as usize;

            let page_size = Size4KiB::SIZE as u64;
            let seg_start_page = (seg_va / page_size) * page_size;
            let seg_end = seg_va + memsz as u64;
            let seg_end_page = ((seg_end + page_size - 1) / page_size) * page_size;
            let total_pages = ((seg_end_page - seg_start_page) / page_size) as usize;

            let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
            if (ph.p_flags & 0x2) != 0 {
                flags |= PageTableFlags::WRITABLE;
            }

            if (ph.p_flags & 0x1) == 0 {
                flags |= PageTableFlags::NO_EXECUTE;
            }

            let (_, _map_end) = self.allocate_pages(seg_start_page, total_pages, flags, &mut user_mapper, frame_allocator);

            if filesz > 0 {
                let mut copied = 0usize;
                while copied < filesz {
                    let va = seg_va + copied as u64;
                    if let Some(phys) = user_mapper.translate_addr(VirtAddr::new(va)) {
                        let kernel_ptr = (phys.as_u64() + phys_offset.as_u64()) as *mut u8;
                        let remaining = filesz - copied;
                        
                        let page_off = (va % page_size) as usize;
                        let to_copy = core::cmp::min(remaining, (page_size as usize) - page_off);

                        unsafe {
                            core::ptr::copy_nonoverlapping(
                                bin.as_ptr().add(file_off + copied),
                                kernel_ptr.add(page_off),
                                to_copy,
                            );
                        }
                        copied += to_copy;
                    } else {
                        panic!("Failed translating va -> phys while copying segment");
                    }
                }
            }

            if memsz > filesz {
                let mut zeroed = 0usize;
                let start_zero = seg_va + filesz as u64;
                let zero_len = memsz - filesz;
                while zeroed < zero_len {
                    let va = start_zero + zeroed as u64;
                    if let Some(phys) = user_mapper.translate_addr(VirtAddr::new(va)) {
                        let kernel_ptr = (phys.as_u64() + phys_offset.as_u64()) as *mut u8;
                        let page_off = (va % page_size) as usize;
                        let to_zero = core::cmp::min(zero_len - zeroed, (page_size as usize) - page_off);
                        unsafe {
                            core::ptr::write_bytes(kernel_ptr.add(page_off), 0, to_zero);
                        }
                        zeroed += to_zero;
                    } else {
                        panic!("Failed translating va -> phys while zeroing bss");
                    }
                }
            }
        } 

        let highest_va = elf.headers.iter()
            .filter(|h| h.p_type == 1)
            .map(|h| h.p_vaddr.as_u64() + h.p_memsz)
            .max()
            .unwrap_or(0);

        let stack_top_va = ((highest_va + Size4KiB::SIZE as u64 + (Size4KiB::SIZE as u64 - 1)) / Size4KiB::SIZE as u64) * Size4KiB::SIZE as u64;
        let stack_pages = 1usize;
        let (_, stack_end) = self.allocate_pages(
            stack_top_va,
            stack_pages,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::USER_ACCESSIBLE | PageTableFlags::NO_EXECUTE,
            &mut user_mapper,
            frame_allocator,
        );


        ctx.rsp = stack_end + Size4KiB::SIZE;
        ctx.rip = elf.entry_point.as_u64();
        ctx.cs = ucs_sel().0 as u64;
        ctx.ss = uds_sel().0 as u64;

        let quant = self.select_quants(&priority);
        
        let kstack = vec![0u8; STACK_SIZE].into_boxed_slice();

        Task { 
            pid: self.get_pid(), 
            ctx, 
            state: TaskState::Running, 
            quant: quant,
            exit_code: 0,
            ticks_left: quant,
            kernel_stack: kstack,
            pt_phys: new_pt.1 
        }
    }


    pub fn create_kernel_task(&mut self, priority: TaskPriority, entry: extern "C" fn()) -> Task {
        let mut ctx = Context::default();

        self.fill_main_registers(&mut ctx);

        let stack = vec![0u8; STACK_SIZE].into_boxed_slice();
        let rsp = stack.as_ptr() as u64 + STACK_SIZE as u64;

        ctx.rsp = rsp;
        ctx.rip = entry as u64;
        ctx.cs = kcs_sel().0 as u64;
        ctx.ss = kds_sel().0 as u64;

        let quant = self.select_quants(&priority);

        Task { 
            pid: self.get_pid(), 
            ctx, 
            state: TaskState::Running, 
            quant: quant,
            exit_code: 0,
            ticks_left: quant,
            kernel_stack: stack,
            pt_phys: unsafe { KERNEL_PT.unwrap() }
        }
    }
}