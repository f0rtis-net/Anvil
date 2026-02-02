use alloc::vec::Vec;
use elf::{ElfBytes, abi::{EM_X86_64, ET_DYN, ET_EXEC, PF_W, PF_X, PT_LOAD}};
use x86_64::{PhysAddr, VirtAddr, structures::paging::{OffsetPageTable, Page, PageTable, PageTableFlags, Size4KiB, page::PageRangeInclusive}};

use crate::arch::amd64::memory::{misc::phys_to_virt, pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order, free_pages}, vmm::{PAGE_SIZE, create_region_for_user_pagetable, map_region_with_table, unmap_region_with_table}};

#[derive(Debug)] pub enum ElfLoadError { 
    InvalidElf, 
    UnsupportedArch, 
    UnsupportedType, 
    NoLoadableSegments, 
    OutOfBounds, 
    AllocFailed, 
}

#[derive(Clone, Copy, Debug)]
pub enum VmaKind {
    ElfSegment,
    UserStack,
    Heap,
    Mmap,
}

pub struct Vma {
    pub kind: VmaKind,
    pub start: VirtAddr,
    pub end: VirtAddr,
    pub flags: PageTableFlags,
    pub frames: Vec<PhysAddr>
}

pub struct AddrSpace {
    pub cr3: PhysAddr,
    pub vmas: Vec<Vma>,

    pub image_start: VirtAddr,
    pub image_end: VirtAddr,
    pub user_stack_top: VirtAddr,
    pub user_stack_bottom: VirtAddr,
}

#[inline]
fn align_down(x: u64, page_size: u64) -> u64 {
    x & !(page_size - 1)
}

#[inline]
fn align_up(x: u64, page_size: u64) -> u64 {
    (x + page_size - 1) & !(page_size - 1)
}

#[inline] fn elf_flags_to_pte(p_flags: u32) -> PageTableFlags { 
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;

    if (p_flags & PF_W) != 0 { 
        flags |= PageTableFlags::WRITABLE; 
    } 

    if (p_flags & PF_X) == 0 { 
        flags |= PageTableFlags::NO_EXECUTE; 
    } 
    
    flags 
}

pub fn make_table_for_cr3(cr3: PhysAddr, hhdm_offset: u64) -> OffsetPageTable<'static> { 
    unsafe { 
        let pml4_virt = phys_to_virt(cr3.as_u64() as usize) as *mut PageTable; 
        let pml4 = &mut *pml4_virt; OffsetPageTable::new(pml4, VirtAddr::new(hhdm_offset)) 
    } 
}

fn page_range_inclusive(start: VirtAddr, end_exclusive: VirtAddr) -> PageRangeInclusive<Size4KiB> {
    let start_page = Page::<Size4KiB>::containing_address(start);
    let end_page = Page::<Size4KiB>::containing_address(VirtAddr::new(end_exclusive.as_u64() - 1));
    PageRangeInclusive { start: start_page, end: end_page }
}

impl AddrSpace {
    pub fn new(cr3: PhysAddr) -> Self {
        Self {
            cr3,
            vmas: Vec::new(),
            image_start: VirtAddr::new(u64::MAX),
            image_end: VirtAddr::new(0),
            user_stack_top: VirtAddr::new(0),
            user_stack_bottom: VirtAddr::new(0),
        }
    }

    pub fn map_anonymous_vma(
        &mut self,
        table: &mut OffsetPageTable,
        kind: VmaKind,
        start: VirtAddr,
        end_exclusive: VirtAddr,
        flags: PageTableFlags,
    ) -> Result<(), &'static str> {
        if end_exclusive.as_u64() <= start.as_u64() {
            return Err("bad vma range");
        }

        let region = page_range_inclusive(start, end_exclusive);

        let mut frames = Vec::<PhysAddr>::new();
        for _ in region {
            let phys = alloc_pages_by_order(0, PAllocFlags::Zeroed).ok_or("alloc failed")?;
            frames.push(phys);
        }

        let region2 = page_range_inclusive(start, end_exclusive);

        map_region_with_table(table, region2, &frames, flags)?;

        self.vmas.push(Vma {
            kind,
            start,
            end: end_exclusive,
            flags,
            frames
        });

        Ok(())
    }

    pub fn destroy(self, hhdm_offset: u64) {
        let mut table = make_table_for_cr3(self.cr3, hhdm_offset);

        for vma in self.vmas {
            let region = page_range_inclusive(vma.start, vma.end);
            let frames = unmap_region_with_table(&mut table, region);

            for phys in frames {
                free_pages(phys);
            }
        }

        free_pages(self.cr3);
    }
}

fn copy_segment_into_vma(
    bytes: &[u8],
    vma: &Vma,
    seg_vaddr: u64,
    seg_filesz: u64,
    seg_offset: u64,
) -> Result<(), ElfLoadError> {
    let file_seg_start = seg_vaddr;
    let file_seg_end = seg_vaddr.checked_add(seg_filesz).ok_or(ElfLoadError::OutOfBounds)?;

    for (i, &frame_base) in vma.frames.iter().enumerate() {
        let page_start = vma.start.as_u64() + (i as u64) * PAGE_SIZE as u64;
        let page_end = page_start + PAGE_SIZE as u64;

        let copy_start = core::cmp::max(page_start, file_seg_start);
        let copy_end = core::cmp::min(page_end, file_seg_end);
        if copy_end <= copy_start {
            continue;
        }

        let within_seg = (copy_start - file_seg_start) as usize;
        let src_off = (seg_offset as usize)
            .checked_add(within_seg)
            .ok_or(ElfLoadError::OutOfBounds)?;
        let len = (copy_end - copy_start) as usize;

        let src_end = src_off.checked_add(len).ok_or(ElfLoadError::OutOfBounds)?;
        if src_end > bytes.len() {
            return Err(ElfLoadError::OutOfBounds);
        }

        let dst_phys = frame_base.as_u64() + (copy_start - page_start);
        let dst_virt = phys_to_virt(dst_phys as usize) as *mut u8;

        unsafe { core::ptr::copy_nonoverlapping(bytes[src_off..src_end].as_ptr(), dst_virt, len); }
    }

    Ok(())
}

pub fn load_elf_into_new_space(
    bytes: &[u8],
    hhdm_offset: u64,
) -> Result<(AddrSpace, VirtAddr), ElfLoadError> {
    let elf = ElfBytes::<elf::endian::LittleEndian>::minimal_parse(bytes)
        .map_err(|_| ElfLoadError::InvalidElf)?;
    let hdr = elf.ehdr;

    if hdr.e_machine != EM_X86_64 {
        return Err(ElfLoadError::UnsupportedArch);
    }
    if hdr.e_type != ET_EXEC && hdr.e_type != ET_DYN {
        return Err(ElfLoadError::UnsupportedType);
    }

    let phdrs = elf.segments().unwrap();

    let cr3 = create_region_for_user_pagetable();
    let mut table = make_table_for_cr3(cr3, hhdm_offset);

    let mut aspace = AddrSpace::new(cr3);

    let mut found = false;
    let mut image_start = u64::MAX;
    let mut image_end = 0u64;

    for ph in phdrs {
        if ph.p_type != PT_LOAD {
            continue;
        }
        found = true;

        let seg_vaddr = ph.p_vaddr;
        let seg_memsz = ph.p_memsz;
        let seg_filesz = ph.p_filesz;
        let seg_offset = ph.p_offset;

        let file_end = seg_offset.checked_add(seg_filesz).ok_or(ElfLoadError::OutOfBounds)?;
        if (file_end as usize) > bytes.len() {
            return Err(ElfLoadError::OutOfBounds);
        }

        image_start = image_start.min(seg_vaddr);
        image_end = image_end.max(seg_vaddr + seg_memsz);

        let flags = elf_flags_to_pte(ph.p_flags);

        let vma_start = VirtAddr::new(align_down(seg_vaddr, PAGE_SIZE as u64));
        let vma_end = VirtAddr::new(align_up(seg_vaddr + seg_memsz, PAGE_SIZE as u64)); // end exclusive

        aspace.map_anonymous_vma(
            &mut table,
            VmaKind::ElfSegment,
            vma_start,
            vma_end,
            flags,
        ).map_err(|_| ElfLoadError::AllocFailed)?; 

        let vma_ref = aspace.vmas.last().unwrap();
        copy_segment_into_vma(bytes, vma_ref, seg_vaddr, seg_filesz, seg_offset)?;
    }

    if !found {
        return Err(ElfLoadError::NoLoadableSegments);
    }

    const USER_STACK_PAGES: usize = 8;
    const USER_STACK_TOP: u64 = 0x0000_7FFF_FFFF_F000;
    const USER_STACK_SIZE: u64 = USER_STACK_PAGES as u64 * PAGE_SIZE as u64;

    let stack_bottom = USER_STACK_TOP - USER_STACK_SIZE;

    let stack_flags = PageTableFlags::PRESENT
        | PageTableFlags::WRITABLE
        | PageTableFlags::USER_ACCESSIBLE
        | PageTableFlags::NO_EXECUTE;

    aspace.map_anonymous_vma(
        &mut table,
        VmaKind::UserStack,
        VirtAddr::new(stack_bottom),
        VirtAddr::new(USER_STACK_TOP),
        stack_flags
    ).map_err(|_| ElfLoadError::AllocFailed)?;

    aspace.image_start = VirtAddr::new(align_down(image_start, PAGE_SIZE as u64));
    aspace.image_end = VirtAddr::new(align_up(image_end, PAGE_SIZE as u64));
    aspace.user_stack_top = VirtAddr::new(USER_STACK_TOP);
    aspace.user_stack_bottom = VirtAddr::new(stack_bottom);

    Ok((aspace, VirtAddr::new(hdr.e_entry)))
}

pub fn unload_process_address_space(aspace: AddrSpace, hhdm_offset: u64) {
    aspace.destroy(hhdm_offset);
}