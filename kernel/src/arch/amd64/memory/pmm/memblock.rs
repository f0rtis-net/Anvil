const MAX_MEMBLOCK_REGIONS: usize = 64;

#[derive(Clone, Copy)]
pub enum RegionPurpose {
    FreeMem,
    AcpiMem,
    ReservedMem
}

#[derive(Clone, Copy)]
pub struct MemblockRegion {
    pub base: u64,
    pub size: u64,
    pub purpose: RegionPurpose,
}

impl MemblockRegion {
    #[inline]
    pub fn end(&self) -> u64 {
        return self.base + self.size;
    }

    pub fn init() -> Self {
        Self {
            base: 0,
            size: 0,
            purpose: RegionPurpose::ReservedMem
        }
    }
}

pub type MemoryRegions = [MemblockRegion; MAX_MEMBLOCK_REGIONS];

pub struct Memblock {
    pub memory: MemoryRegions,
    pub mem_cnt: usize,

    pub reserved: MemoryRegions,
    pub reserved_cnt: usize
}

impl Memblock {
    pub fn new() -> Self {
        Self {
            memory: [MemblockRegion::init(); MAX_MEMBLOCK_REGIONS],
            mem_cnt: 0,
            reserved: [MemblockRegion::init(); MAX_MEMBLOCK_REGIONS],
            reserved_cnt: 0
        }
    }

    pub fn add_usable(&mut self, base: u64, size: u64) -> Option<()> {
        if size == 0 || self.mem_cnt >= MAX_MEMBLOCK_REGIONS {
            return None;
        }
        
        self.memory[self.mem_cnt] = MemblockRegion { 
            base, 
            size,
            purpose: RegionPurpose::FreeMem
        };

        self.mem_cnt += 1;

        return Some(());
    }

    pub fn add_reserved(&mut self, base: u64, size: u64, is_acpi_recl: bool) -> Option<()> {
        if size == 0 || self.reserved_cnt >= MAX_MEMBLOCK_REGIONS {
            return None;
        }

        self.reserved[self.reserved_cnt] = MemblockRegion { 
            base, 
            size,
            purpose: if is_acpi_recl { RegionPurpose::AcpiMem } else { RegionPurpose::ReservedMem }
        };

        self.reserved_cnt += 1;

        return Some(());
    }    

    pub fn reserve_from_usable(
        &mut self,
        base: u64,
        size: u64,
        is_acpi_recl: bool,
    ) -> Option<()> {
        if size == 0 {
            return None;
        }

        let end = base + size;

        let mut i = 0;
        while i < self.mem_cnt {
            let region = self.memory[i];
            let r_start = region.base;
            let r_end   = region.end();

            if end <= r_start || base >= r_end {
                i += 1;
                continue;
            }

            if base <= r_start && end >= r_end {
                self.remove_memory_region(i);
                continue;
            }

            if base > r_start && end < r_end {
                 if self.mem_cnt >= MAX_MEMBLOCK_REGIONS {
                    return None; 
                }
                self.memory[self.mem_cnt] = MemblockRegion {
                    base: end,
                    size: r_end - end,
                    purpose: RegionPurpose::FreeMem,
                };
                self.mem_cnt += 1;

                self.memory[i] = MemblockRegion {
                    base: r_start,
                    size: base - r_start,
                    purpose: RegionPurpose::FreeMem,
                };
                i += 1;
                continue;
            }

            if base <= r_start {
                self.memory[i] = MemblockRegion {
                    base: end,
                    size: r_end - end,
                    purpose: RegionPurpose::FreeMem,
                };
                i += 1;
                continue;
            }

            if end >= r_end {
                self.memory[i] = MemblockRegion {
                    base: r_start,
                    size: base - r_start,
                    purpose: RegionPurpose::FreeMem,
                };
                i += 1;
                continue;
            }

            i += 1;
        }

        self.add_reserved(base, size, is_acpi_recl)
    }

    fn remove_memory_region(&mut self, idx: usize) {
        debug_assert!(idx < self.mem_cnt);
        self.mem_cnt -= 1;
        self.memory[idx] = self.memory[self.mem_cnt];
        self.memory[self.mem_cnt] = MemblockRegion::init();
    }
}