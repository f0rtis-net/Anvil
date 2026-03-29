use alloc::{collections::BTreeMap, vec::Vec};
use x86_64::{PhysAddr, VirtAddr, structures::paging::{OffsetPageTable, PageTable, PageTableFlags}};
use crate::arch::amd64::memory::{misc::virt_to_phys, pmm::pages_allocator::free_pages, vmm::{
    PAGE_SIZE, map_single_page, unmap_single_page
}};

bitflags::bitflags! {
    #[derive(Clone, Copy)]
    pub struct MapFlags: u32 {
        const READ    = 1 << 0;
        const WRITE   = 1 << 1;
        const EXEC    = 1 << 2;
        const USER    = 1 << 3;
        const NOCACHE = 1 << 4;
    }
}

impl MapFlags {
    pub fn to_page_table_flags(&self) -> PageTableFlags { 
        let mut f = PageTableFlags::PRESENT;
        if self.contains(Self::WRITE)   { f |= PageTableFlags::WRITABLE; }
        if self.contains(Self::USER)    { f |= PageTableFlags::USER_ACCESSIBLE; }
        if self.contains(Self::NOCACHE) { f |= PageTableFlags::NO_CACHE; }
        if !self.contains(Self::EXEC)   { f |= PageTableFlags::NO_EXECUTE; }
        f
    }
}

pub enum VmaBacking {
    Physical { phys_addr: PhysAddr },
    Device   { phys_addr: PhysAddr },
    Reserved,
}

pub struct Vma {
    pub vaddr:   VirtAddr,
    pub size:    usize,
    pub flags:   MapFlags,
    pub backing: VmaBacking,
}

impl Vma {
    pub fn end(&self) -> VirtAddr {
        VirtAddr::new(self.vaddr.as_u64() + self.size as u64)
    }
    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.vaddr && addr < self.end()
    }
    pub fn overlaps(&self, other: &Vma) -> bool {
        self.vaddr < other.end() && other.vaddr < self.end()
    }
}

pub struct AddrSpace {
    vmas:       BTreeMap<u64, Vma>,   
    pub page_table: OffsetPageTable<'static>,
}

#[derive(Debug)]
pub enum VmaError {
    NotAligned,
    Overlap,
    NotFound,
    PageTableError(&'static str),
}

impl AddrSpace {
    pub fn new(page_table: OffsetPageTable<'static>) -> Self {
        Self { vmas: BTreeMap::new(), page_table }
    }

    pub fn get_page_table_phys(&self) -> PhysAddr {
        let virt = self.page_table.level_4_table() as *const PageTable as u64;
        PhysAddr::new(virt_to_phys(virt as usize) as u64)
    }

    pub fn map(
        &mut self,
        vaddr:   VirtAddr,
        size:    usize,
        backing: VmaBacking,
        flags:   MapFlags,
    ) -> Result<(), VmaError> {
        if !vaddr.is_aligned(PAGE_SIZE as u64) || size % PAGE_SIZE != 0 {
            return Err(VmaError::NotAligned);
        }

        let vma = Vma { vaddr, size, flags, backing };

        if self.find_overlapping(&vma).is_some() {
            return Err(VmaError::Overlap);
        }

        self.map_in_page_table(&vma)
            .map_err(VmaError::PageTableError)?;

        self.vmas.insert(vaddr.as_u64(), vma);
        Ok(())
    }

    pub fn unmap(&mut self, vaddr: VirtAddr) -> Result<(), VmaError> {
        let vma = self.vmas.remove(&vaddr.as_u64())
            .ok_or(VmaError::NotFound)?;
        
        let pages = vma.size / PAGE_SIZE;
        for i in 0..pages {
            let va = VirtAddr::new(vma.vaddr.as_u64() + (i * PAGE_SIZE) as u64);
            match &vma.backing {
                VmaBacking::Reserved => {
                    let _ = unmap_single_page(&mut self.page_table, va);
                }
                VmaBacking::Physical { .. } | VmaBacking::Device { .. } => {
                    unmap_single_page(&mut self.page_table, va)
                        .map_err(VmaError::PageTableError)?;
                }
            }
        }
        Ok(())
    }

    pub fn protect(
        &mut self,
        vaddr: VirtAddr,
        flags: MapFlags,
    ) -> Result<(), VmaError> {
        let vma = self.vmas.get_mut(&vaddr.as_u64())
            .ok_or(VmaError::NotFound)?;

        let pt_flags = flags.to_page_table_flags();
        let pages = vma.size / PAGE_SIZE;

        for i in 0..pages {
            let va = VirtAddr::new(vma.vaddr.as_u64() + (i * PAGE_SIZE) as u64);

            let phys = match &vma.backing {
                VmaBacking::Physical { phys_addr } |
                VmaBacking::Device   { phys_addr } => {
                    PhysAddr::new(phys_addr.as_u64() + (i * PAGE_SIZE) as u64)
                }
                VmaBacking::Reserved => continue,
            };

            unmap_single_page(&mut self.page_table, va)
                .map_err(VmaError::PageTableError)?;

            map_single_page(&mut self.page_table, va, phys, pt_flags)
                .map_err(VmaError::PageTableError)?;
        }

        vma.flags = flags;
        Ok(())
    }

    pub fn find(&self, addr: VirtAddr) -> Option<&Vma> {
        self.vmas
            .range(..=addr.as_u64())
            .next_back()
            .map(|(_, vma)| vma)
            .filter(|vma| vma.contains(addr))
    }

    fn map_in_page_table(&mut self, vma: &Vma) -> Result<(), &'static str> {
        let pages    = vma.size / PAGE_SIZE;
        let pt_flags = vma.flags.to_page_table_flags();

        for i in 0..pages {
            let va = VirtAddr::new(vma.vaddr.as_u64() + (i * PAGE_SIZE) as u64);

            match &vma.backing {
                VmaBacking::Physical { phys_addr } |
                VmaBacking::Device   { phys_addr } => {
                    let pa = PhysAddr::new(phys_addr.as_u64() + (i * PAGE_SIZE) as u64);
                    map_single_page(&mut self.page_table, va, pa, pt_flags)?;
                }
                VmaBacking::Reserved => {
                }
            }
        }
        Ok(())
    }

    fn find_overlapping(&self, new: &Vma) -> Option<&Vma> {
        self.vmas
            .range(..new.end().as_u64())
            .next_back()
            .map(|(_, vma)| vma)
            .filter(|vma| vma.overlaps(new))
    }
}

impl Drop for AddrSpace {
    fn drop(&mut self) {
        let vaddrs: Vec<u64> = self.vmas.keys().copied().collect();
        
        for vaddr in vaddrs {
            let vma = self.vmas.remove(&vaddr).unwrap();
            let pages = vma.size / PAGE_SIZE;
            
            for i in 0..pages {
                let va = VirtAddr::new(vma.vaddr.as_u64() + (i * PAGE_SIZE) as u64);
                
                match &vma.backing {
                    VmaBacking::Physical { .. } => {
                        if let Ok(pa) = unmap_single_page(&mut self.page_table, va) {
                            free_pages(pa);
                        }
                    }
                    VmaBacking::Device { .. } => {
                        let _ = unmap_single_page(&mut self.page_table, va);
                    }
                    VmaBacking::Reserved => {
                        if let Ok(pa) = unmap_single_page(&mut self.page_table, va) {
                            free_pages(pa);
                        }
                    }
                }
            }
        }
        
        let pt_phys = self.get_page_table_phys();
        free_pages(pt_phys);
    }
}