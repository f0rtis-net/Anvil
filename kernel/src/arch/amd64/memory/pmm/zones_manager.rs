use core::ptr::NonNull;
use limine::memory_map::{Entry, EntryType};

use crate::{
    arch::amd64::memory::{
        misc::{align_up, phys_to_virt},
        pmm::{
            early_allocator::BumpState,
            frame_area::{
                AreaId, FRAME_SIZE, Frame, FramesArea, frames_db, initialize_page_database, physical_to_pfn
            },
            mem_zones::{MemoryZone, MemoryZoneType},
            memblock::Memblock,
        },
    },
    serial_println,
};

pub struct MemZonesManager {
    highmem: NonNull<MemoryZone>,
    dma: NonNull<MemoryZone>,
}

unsafe impl Send for MemZonesManager {}
unsafe impl Sync for MemZonesManager {}

impl MemZonesManager {
    pub fn init(highmem_zone: NonNull<MemoryZone>, dma_zone: NonNull<MemoryZone>) -> Self {
        Self {
            highmem: highmem_zone,
            dma: dma_zone,
        }
    }

    pub fn get_highmem_zone(&mut self) -> &mut MemoryZone {
        unsafe { self.highmem.as_mut() }
    }

    pub fn get_dma_zone(&mut self) -> &mut MemoryZone {
        unsafe { self.dma.as_mut() }
    }
}

fn init_zone_from_memblock(
    mem: &mut Memblock,
    bump: &mut BumpState,
    zone_type: MemoryZoneType,
    max_bytes: Option<usize>,
) -> NonNull<MemoryZone> {
    let mut zone = MemoryZone::new(zone_type);

    let mut used_bytes = 0usize;
    let mut i = 0;

    while i < mem.mem_cnt {
        let region = mem.memory[i];

        let phys_start = align_up(region.base as usize, FRAME_SIZE);
        let phys_end = (region.base + region.size) as usize & !(FRAME_SIZE - 1);

        if phys_start >= phys_end {
            i += 1;
            continue;
        }

        let region_bytes = phys_end - phys_start;

        if let Some(limit) = max_bytes {
            if used_bytes >= limit {
                break;
            }
        }

        let mut take_bytes = match max_bytes {
            Some(limit) => core::cmp::min(region_bytes, limit - used_bytes),
            None => region_bytes,
        };

        take_bytes &= !(FRAME_SIZE - 1);
        if take_bytes == 0 {
            i += 1;
            continue;
        }

        mem.reserve_from_usable(phys_start as u64, take_bytes as u64, false)
            .expect("reserve_from_usable(zone) failed");

        let base_pfn = physical_to_pfn(phys_start);
        let page_cnt = take_bytes / FRAME_SIZE;

        if page_cnt == 0 {
            i += 1;
            continue;
        }

        let meta_bytes = page_cnt * core::mem::size_of::<Frame>();
        let meta_ptr = bump
            .alloc_zeroed(meta_bytes, core::mem::align_of::<Frame>())
            .expect("OOM: frame metadata");

        let mut area = FramesArea::empty();
        area.init(
            meta_ptr.as_ptr() as *mut Frame,
            zone_type,
            base_pfn,
            page_cnt,
        );

        let area_id: AreaId = {
            let mut db = frames_db();
            db.add_area(area)
        };

        zone.add_area(area_id);

        used_bytes += take_bytes;
    }

    zone.init();

    let zone_ptr = bump
        .alloc_zeroed(
            core::mem::size_of::<MemoryZone>(),
            core::mem::align_of::<MemoryZone>(),
        )
        .expect("OOM: MemoryZone")
        .as_ptr() as *mut MemoryZone;

    unsafe {
        zone_ptr.write(zone);
        NonNull::new_unchecked(zone_ptr)
    }
}

fn find_largest_free_region<'a>(mmap: &'a [&'a Entry]) -> Option<(u64, u64)> {
    let mut largest_entry: Option<&Entry> = None;
    let mut largest_size: u64 = 0;

    for entry in mmap {
        if entry.entry_type == EntryType::USABLE && entry.length > largest_size {
            largest_size = entry.length;
            largest_entry = Some(*entry);
        }
    }

    largest_entry.map(|e| (e.base, e.length))
}

fn collect_regions<'a>(mmap: &'a [&'a Entry]) -> Memblock {
    let mut memblock = Memblock::new();

    for entry in mmap {
        if entry.entry_type == EntryType::USABLE {
            memblock.add_usable(entry.base, entry.length);
        } else {
            memblock.add_reserved(
                entry.base,
                entry.length,
                matches!(entry.entry_type, EntryType::ACPI_RECLAIMABLE),
            );
        }
    }

    memblock
}

fn estimate_bump_bytes_after_reserve(mem: &Memblock) -> usize {
    let mut bytes: usize = 0;

    for i in 0..mem.mem_cnt {
        let r = mem.memory[i];

        let phys_start = align_up(r.base as usize, FRAME_SIZE);
        let phys_end = (r.base + r.size) as usize & !(FRAME_SIZE - 1);

        if phys_start >= phys_end {
            continue;
        }

        let page_cnt = (phys_end - phys_start) / FRAME_SIZE;
        if page_cnt == 0 {
            continue;
        }

        let meta = page_cnt * core::mem::size_of::<Frame>();
        bytes = align_up(bytes, core::mem::align_of::<Frame>());
        bytes = bytes.saturating_add(meta);
    }

    bytes = align_up(bytes, 16);
    bytes = bytes.saturating_add(core::mem::size_of::<MemZonesManager>());
    bytes = bytes.saturating_add(16 * 1024);

    align_up(bytes, FRAME_SIZE)
}

pub fn init_memory_zones<'a>(hhdm: usize, mmap: &'a [&'a Entry]) -> MemZonesManager {
    serial_println!("Initializing memory zone manager...");

    serial_println!("Initializing page database...");
    initialize_page_database();
    serial_println!("Page database initialized!");

    let mut collected_regions = collect_regions(mmap);

    let (largest_base, largest_len) =
        find_largest_free_region(mmap).expect("No usable memory regions");

    let bump_size = estimate_bump_bytes_after_reserve(&collected_regions);
    let bump_phys_start = align_up(largest_base as usize, FRAME_SIZE);

    serial_println!("Needed for metadata: {} MiB", bump_size / 1024 / 1024);

    if bump_size as u64 > largest_len {
        panic!("Largest usable region too small for bump");
    }

    let bump_phys_end = bump_phys_start + bump_size;

    collected_regions
        .reserve_from_usable(bump_phys_start as u64, bump_size as u64, false)
        .expect("reserve_from_usable(bump) failed");

    let bump_virt_start = phys_to_virt(hhdm, bump_phys_start);
    let bump_virt_end = phys_to_virt(hhdm, bump_phys_end);

    let mut bump = BumpState::init(bump_virt_start, bump_virt_end);

    serial_println!("Allocating memory for dma zone...");
    const DMA_ZONE_SIZE: usize = 16 * 1024 * 1024;
    let dma_zone = init_zone_from_memblock(
        &mut collected_regions,
        &mut bump,
        MemoryZoneType::Dma,
        Some(DMA_ZONE_SIZE),
    );

    serial_println!("Allocating memory for highmem zone...");
    let highmem_zone = init_zone_from_memblock(
        &mut collected_regions,
        &mut bump,
        MemoryZoneType::High,
        None,
    );

    let manager = MemZonesManager::init(highmem_zone, dma_zone);
    
    serial_println!("Memory zone manager initialized!");
    manager
}
