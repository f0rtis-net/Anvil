use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use bitflags::bitflags;
use x86_64::{
    PhysAddr, VirtAddr,
    structures::paging::{Page, PageTableFlags, Size4KiB, page::PageRangeInclusive},
};

const USER_SPACE_START: u64 = 0x0000_0000_0040_0000; 
const USER_SPACE_END:   u64 = 0x0000_7FFF_FFFF_0000;

bitflags! {
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct VmFlags: u32 {
        const READ    = 1 << 0;
        const WRITE   = 1 << 1;
        const EXEC    = 1 << 2;
        const FIXED   = 1 << 3;
    }
}

impl VmFlags {
    pub fn to_page_table_flags(self) -> PageTableFlags {
        let mut flags = PageTableFlags::PRESENT | PageTableFlags::USER_ACCESSIBLE;
        if self.contains(VmFlags::WRITE) {
            flags |= PageTableFlags::WRITABLE;
        }
        if !self.contains(VmFlags::EXEC) {
            flags |= PageTableFlags::NO_EXECUTE;
        }
        flags
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum VmAreaKind {
    Text,       
    Data,       
    Bss,        
    Stack,      
    Heap,       
    Mmap,       
}

#[derive(Debug, Clone)]
pub struct VmArea {
    pub start:   VirtAddr,
    pub end:     VirtAddr,
    pub flags:   VmFlags,
    pub kind:    VmAreaKind,
    pub name:    Option<&'static str>,
}

impl VmArea {
    pub fn new(start: VirtAddr, end: VirtAddr, flags: VmFlags, kind: VmAreaKind) -> Self {
        Self { start, end, flags, kind, name: None }
    }

    pub fn with_name(mut self, name: &'static str) -> Self {
        self.name = Some(name);
        self
    }

    pub fn size(&self) -> u64 {
        self.end.as_u64() - self.start.as_u64()
    }

    pub fn contains(&self, addr: VirtAddr) -> bool {
        addr >= self.start && addr < self.end
    }

    pub fn overlaps(&self, other: &VmArea) -> bool {
        self.start < other.end && other.start < self.end
    }

    pub fn is_valid(&self) -> bool {
        self.start < self.end
            && self.end.as_u64() <= USER_SPACE_END
            && self.start.as_u64() >= 0x1000
    }

    pub fn page_range(&self) -> PageRangeInclusive {
        let start_page = Page::<Size4KiB>::containing_address(self.start);
        let end_page   = Page::<Size4KiB>::containing_address(self.end - 1u64);
        Page::range_inclusive(start_page, end_page)
    }

    pub fn page_table_flags(&self) -> PageTableFlags {
        self.flags.to_page_table_flags()
    }
}

#[derive(Debug)]
pub struct AddrSpace {
    areas:          Vec<VmArea>,
    pub pml4_phys:  PhysAddr,
    pub brk:        VirtAddr,
    pub brk_start:  VirtAddr,
}

impl AddrSpace {
    pub fn new(pml4_phys: PhysAddr) -> Self {
        Self {
            areas:     Vec::new(),
            pml4_phys,
            brk:       VirtAddr::new(0),
            brk_start: VirtAddr::new(0),
        }
    }

    pub fn add_area(&mut self, area: VmArea) -> Result<(), &'static str> {
        if !area.is_valid() {
            return Err("VmArea: invalid range or outside user space");
        }
        if self.overlaps_any(&area) {
            return Err("VmArea: overlaps with existing region");
        }

        let pos = self.areas.partition_point(|a| a.start <= area.start);
        self.areas.insert(pos, area);
        Ok(())
    }

    pub fn remove_area(&mut self, start: VirtAddr) -> Option<VmArea> {
        let pos = self.areas.iter().position(|a| a.start == start)?;
        Some(self.areas.remove(pos))
    }


    pub fn find_area(&self, addr: VirtAddr) -> Option<&VmArea> {
        let idx = self.areas.partition_point(|a| a.start <= addr);
        if idx == 0 {
            return None;
        }
        let area = &self.areas[idx - 1];
        if area.contains(addr) { Some(area) } else { None }
    }

    pub fn find_area_mut(&mut self, addr: VirtAddr) -> Option<&mut VmArea> {
        let idx = self.areas.partition_point(|a| a.start <= addr);
        if idx == 0 {
            return None;
        }
        let area = &mut self.areas[idx - 1];
        if area.contains(addr) { Some(area) } else { None }
    }

    pub fn find_free_region(&self, size: u64, align: u64) -> Option<VirtAddr> {
        let mut cursor = align_up_u64(USER_SPACE_START, align);

        for area in &self.areas {
            let candidate_end = cursor + size;
            if candidate_end <= area.start.as_u64() {
                return Some(VirtAddr::new(cursor));
            }
            cursor = align_up_u64(area.end.as_u64(), align);
        }

        if cursor + size <= USER_SPACE_END {
            Some(VirtAddr::new(cursor))
        } else {
            None
        }
    }

    pub fn init_brk(&mut self, start: VirtAddr) {
        self.brk_start = start;
        self.brk       = start;
    }

    pub fn set_brk(&mut self, new_brk: VirtAddr) -> Result<VirtAddr, &'static str> {
        if new_brk < self.brk_start {
            return Err("brk: cannot move below brk_start");
        }
        if new_brk.as_u64() > USER_SPACE_END {
            return Err("brk: exceeds user space limit");
        }

        if new_brk > self.brk {
            let candidate = VmArea::new(self.brk, new_brk, VmFlags::READ | VmFlags::WRITE, VmAreaKind::Heap);
            if self.overlaps_any(&candidate) {
                return Err("brk: would overlap existing region");
            }
        }

        self.brk = new_brk;
        Ok(self.brk)
    }

    pub fn handle_page_fault(
        &self,
        addr: VirtAddr,
        write: bool,
        user: bool,
    ) -> Result<PageTableFlags, PageFaultReject> {
        if !user {
            return Err(PageFaultReject::KernelAccess);
        }

        let area = self.find_area(addr).ok_or(PageFaultReject::NoMapping)?;

        if write && !area.flags.contains(VmFlags::WRITE) {
            return Err(PageFaultReject::WriteToReadOnly);
        }

        Ok(area.page_table_flags())
    }

    pub fn areas(&self) -> &[VmArea] {
        &self.areas
    }

    pub fn all_page_ranges(&self) -> impl Iterator<Item = PageRangeInclusive> + '_ {
        self.areas.iter().map(|a| a.page_range())
    }

    pub fn validate(&self) -> Result<(), &'static str> {
        if self.pml4_phys.is_null() {
            return Err("pml4_phys is null");
        }
        for window in self.areas.windows(2) {
            let (a, b) = (&window[0], &window[1]);
            if !a.is_valid() {
                return Err("invalid VmArea detected");
            }
            if a.end > b.start {
                return Err("overlapping regions detected");
            }
        }
        Ok(())
    }

    pub fn clear(&mut self) {
        self.areas.clear();
        self.brk       = VirtAddr::new(0);
        self.brk_start = VirtAddr::new(0);
    }


    fn overlaps_any(&self, new_area: &VmArea) -> bool {
        let idx = self.areas.partition_point(|a| a.end.as_u64() <= new_area.start.as_u64());
        self.areas[idx..].iter().any(|a| {
            if a.start >= new_area.end { false } else { a.overlaps(new_area) }
        })
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PageFaultReject {
    NoMapping,        
    WriteToReadOnly,  
    KernelAccess,     
}

fn align_up_u64(val: u64, align: u64) -> u64 {
    debug_assert!(align.is_power_of_two(), "align must be power of two");
    (val + align - 1) & !(align - 1)
}

impl Default for AddrSpace {
    fn default() -> Self {
        Self::new(PhysAddr::zero())
    }
}