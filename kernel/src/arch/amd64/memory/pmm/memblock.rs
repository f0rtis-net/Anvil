#![allow(dead_code)]

use limine::memory_map::{Entry, EntryType};

use crate::{arch::amd64::memory::misc::human_readable_size, serial_println};

pub const MAX_MEMBLOCK_REGIONS: usize = 64;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemblockType {
    Usable,
    Reserved,
    AcpiReclaim,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MemblockRegion {
    pub base: u64,
    pub size: u64,
    pub kind: MemblockType,
}

impl MemblockRegion {
    #[inline]
    pub const fn empty() -> Self {
        Self { base: 0, size: 0, kind: MemblockType::Reserved }
    }

    #[inline]
    pub fn end(&self) -> u64 {
        self.base.saturating_add(self.size)
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        self.size == 0
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum MemblockError {
    OutOfSlots,
    ZeroSize,
    Overflow,
    InvalidKind,
}

pub struct Memblock {
    memory: [MemblockRegion; MAX_MEMBLOCK_REGIONS],
    mem_cnt: usize,

    reserved: [MemblockRegion; MAX_MEMBLOCK_REGIONS],
    res_cnt: usize,
}

struct MemblockStats {
    usable_regions: usize,

    usable_bytes: u64,

    usable_min_addr: Option<u64>,
    usable_max_addr: Option<u64>,
}

fn collect_memblock_stats(memblock: &Memblock) -> MemblockStats {
    let mut stats = MemblockStats {
        usable_regions: memblock.mem_cnt,
        usable_bytes: 0,
        usable_min_addr: None,
        usable_max_addr: None,
    };

    // usable
    for m in memblock.memory_regions() {
        stats.usable_bytes += m.size;

        stats.usable_min_addr = Some(match stats.usable_min_addr {
            None => m.base,
            Some(x) => core::cmp::min(x, m.base),
        });

        stats.usable_max_addr = Some(match stats.usable_max_addr {
            None => m.end(),
            Some(x) => core::cmp::max(x, m.end()),
        });
    }

    for r in memblock.reserved_regions() {
        let mut bytes = 0;

        for m in memblock.memory_regions() {
            let lo = core::cmp::max(r.base, m.base);
            let hi = core::cmp::min(r.end(), m.end());

            if lo < hi {
                bytes += hi - lo;
            }
        }

        if bytes == 0 {
            continue;
        }
    }

    stats
}


fn compact_list(arr: &mut [MemblockRegion; MAX_MEMBLOCK_REGIONS], cnt: &mut usize) {
    let mut w = 0;
    for i in 0..*cnt {
        if arr[i].size == 0 {
            continue;
        }
        if w != i {
            arr[w] = arr[i];
        }
        w += 1;
    }
    for i in w..*cnt {
        arr[i] = MemblockRegion::empty();
    }
    *cnt = w;
}

fn sort_by_base(arr: &mut [MemblockRegion; MAX_MEMBLOCK_REGIONS], cnt: usize) {
    let mut i = 1;
    while i < cnt {
        let key = arr[i];
        let mut j = i;
        while j > 0 && arr[j - 1].base > key.base {
            arr[j] = arr[j - 1];
            j -= 1;
        }
        arr[j] = key;
        i += 1;
    }
}

fn merge_overlaps_same_kind(arr: &mut [MemblockRegion; MAX_MEMBLOCK_REGIONS], cnt: &mut usize) {
    if *cnt == 0 {
        return;
    }

    let mut w = 0;
    let mut cur = arr[0];

    for i in 1..*cnt {
        let r = arr[i];
        if r.kind == cur.kind && r.base <= cur.end() {
            let new_end = core::cmp::max(cur.end(), r.end());
            cur.size = new_end - cur.base;
        } else {
            arr[w] = cur;
            w += 1;
            cur = r;
        }
    }

    arr[w] = cur;
    w += 1;

    for i in w..*cnt {
        arr[i] = MemblockRegion::empty();
    }
    *cnt = w;
}

fn subtract_reserved(
    memory: &[MemblockRegion],
    mem_cnt: usize,
    reserved: &[MemblockRegion],
    res_cnt: usize,
    out: &mut [MemblockRegion; MAX_MEMBLOCK_REGIONS],
) -> Result<usize, MemblockError> {
    let mut out_cnt = 0;
    let mut ri = 0;

    for mi in 0..mem_cnt {
        let m = memory[mi];
        let mut cur = m.base;
        let mend = m.end();

        while ri < res_cnt && reserved[ri].end() <= cur {
            ri += 1;
        }

        let mut rj = ri;
        while rj < res_cnt {
            let r = reserved[rj];
            if r.base >= mend {
                break;
            }
            if r.end() <= cur {
                rj += 1;
                continue;
            }

            let left_end = core::cmp::min(r.base, mend);
            if left_end > cur {
                if out_cnt >= MAX_MEMBLOCK_REGIONS {
                    return Err(MemblockError::OutOfSlots);
                }
                out[out_cnt] = MemblockRegion {
                    base: cur,
                    size: left_end - cur,
                    kind: MemblockType::Usable,
                };
                out_cnt += 1;
            }

            if r.end() >= mend {
                cur = mend;
                break;
            }
            cur = r.end();
            rj += 1;
        }

        if cur < mend {
            if out_cnt >= MAX_MEMBLOCK_REGIONS {
                return Err(MemblockError::OutOfSlots);
            }
            out[out_cnt] = MemblockRegion {
                base: cur,
                size: mend - cur,
                kind: MemblockType::Usable,
            };
            out_cnt += 1;
        }
    }

    Ok(out_cnt)
}

impl Memblock {
    pub const fn new() -> Self {
        Self {
            memory: [MemblockRegion::empty(); MAX_MEMBLOCK_REGIONS],
            mem_cnt: 0,
            reserved: [MemblockRegion::empty(); MAX_MEMBLOCK_REGIONS],
            res_cnt: 0,
        }
    }

    #[inline]
    pub fn memory_regions(&self) -> &[MemblockRegion] {
        &self.memory[..self.mem_cnt]
    }

    #[inline]
    pub fn reserved_regions(&self) -> &[MemblockRegion] {
        &self.reserved[..self.res_cnt]
    }

    pub fn max_phys_addr(&self) -> u64 {
        let mut max_end = 0u64;

        for i in 0..self.mem_cnt {
            let e = self.memory[i].end();
            if e > max_end { max_end = e; }
        }
        for i in 0..self.res_cnt {
            let e = self.reserved[i].end();
            if e > max_end { max_end = e; }
        }

        max_end
    }

    pub fn add_memory(&mut self, base: u64, size: u64) -> Result<(), MemblockError> {
        if size == 0 {
            return Err(MemblockError::ZeroSize);
        }
        let end = base.checked_add(size).ok_or(MemblockError::Overflow)?;

        if self.mem_cnt >= MAX_MEMBLOCK_REGIONS {
            return Err(MemblockError::OutOfSlots);
        }

        self.memory[self.mem_cnt] = MemblockRegion {
            base,
            size: end - base,
            kind: MemblockType::Usable,
        };
        self.mem_cnt += 1;
        Ok(())
    }

    pub fn add_reserved(&mut self, base: u64, size: u64, kind: MemblockType) -> Result<(), MemblockError> {
        if kind == MemblockType::Usable {
            return Err(MemblockError::InvalidKind);
        }
        if size == 0 {
            return Err(MemblockError::ZeroSize);
        }
        let end = base.checked_add(size).ok_or(MemblockError::Overflow)?;

        if self.res_cnt >= MAX_MEMBLOCK_REGIONS {
            return Err(MemblockError::OutOfSlots);
        }

        self.reserved[self.res_cnt] = MemblockRegion {
            base,
            size: end - base,
            kind,
        };
        self.res_cnt += 1;
        Ok(())
    }

    pub fn normalize(&mut self) -> Result<(), MemblockError> {
        compact_list(&mut self.memory, &mut self.mem_cnt);
        compact_list(&mut self.reserved, &mut self.res_cnt);

        sort_by_base(&mut self.memory, self.mem_cnt);
        merge_overlaps_same_kind(&mut self.memory, &mut self.mem_cnt);

        sort_by_base(&mut self.reserved, self.res_cnt);
        merge_overlaps_same_kind(&mut self.reserved, &mut self.res_cnt);

        let mut new_memory = [MemblockRegion::empty(); MAX_MEMBLOCK_REGIONS];
        let new_cnt = subtract_reserved(
            &self.memory,
            self.mem_cnt,
            &self.reserved,
            self.res_cnt,
            &mut new_memory,
        )?;

        self.memory = new_memory;
        self.mem_cnt = new_cnt;

        compact_list(&mut self.memory, &mut self.mem_cnt);
        sort_by_base(&mut self.memory, self.mem_cnt);
        merge_overlaps_same_kind(&mut self.memory, &mut self.mem_cnt);

        Ok(())
    }
}

fn memblock_statistics(memblock: &Memblock) {
    let stats = collect_memblock_stats(memblock);

    let usable_size = human_readable_size(stats.usable_bytes);

    serial_println!("\n============ Memblock summary ============");
    serial_println!("Usable regions count:          {}", stats.usable_regions);
    serial_println!(
        "Usable memory total:           {} {}",
        usable_size.value,
        usable_size.unit.as_str()
    );

    if let (Some(lo), Some(hi)) = (stats.usable_min_addr, stats.usable_max_addr) {
        serial_println!(
            "Usable physical address range: [{:#x} .. {:#x})",
            lo,
            hi
        );
    }

    serial_println!("-----------------------------------------");
    serial_println!("Memory passed to PMM:");
    serial_println!("  Regions count: {}", stats.usable_regions);
    serial_println!(
        "  Total size:    {} {}",
        usable_size.value,
        usable_size.unit.as_str()
    );

    serial_println!("============ Memblock summary ============\n");
}


pub fn initialize_memblock_from_mm<'a>(mmap: &'a[&'a Entry]) -> Result<Memblock, MemblockError> {
    serial_println!("Initializing memblock manager...");

    let mut memblock = Memblock::new();

    for entry in mmap {
        match entry.entry_type {
            EntryType::USABLE => memblock.add_memory(entry.base, entry.length)?,
            EntryType::ACPI_RECLAIMABLE => memblock.add_reserved(entry.base, entry.length, MemblockType::AcpiReclaim)?,
            _ => memblock.add_reserved(entry.base, entry.length, MemblockType::Reserved)?
        };
    }

    memblock_statistics(&memblock);

    serial_println!("Memblock manager initialized!");

    Ok(memblock)
}