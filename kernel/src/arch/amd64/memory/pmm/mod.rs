use limine::memory_map::{Entry, EntryType};
use spin::{Mutex, Once};

use crate::{arch::amd64::memory::pmm::frame_alloc::{Buddy, pfn_to_physical}, serial_println};

pub mod frame_alloc;
pub mod kheap;

static BUDDY: Once<Mutex<Buddy>> = Once::new();

fn find_largest_free_region<'a>(mmap: &'a[&'a Entry]) -> Option<(u64, u64)> {  
    let mut largest_entry = None;
    let mut largest_size = 0;
    
    for entry in mmap {
        if entry.entry_type == EntryType::USABLE {
            if entry.length > largest_size {
                largest_size = entry.length;
                largest_entry = Some(entry);
            }
        }
    }
    
    largest_entry.map(|e| (e.base, e.length))
}

pub fn init_physical_memory(hhdm_offset: u64, mmap: &[&Entry]) {
    serial_println!("Initializing physical memory manager...");

    let (phys_base, region_size) =
        find_largest_free_region(mmap)
            .expect("No usable memory region");

    let phys_base = phys_base as usize;
    let region_size = region_size as usize;

    let phys_end = phys_base + region_size;

    let buddy_phys_start = phys_base;
    let buddy_phys_end   = phys_end;

    //rework this moment. Add optimization - buddy alloc needs for metadata found_mem - heap size. Not full mem, cuz at this way we have excess
    let needed_heap =
        Buddy::calculate_needed_heap(buddy_phys_start, buddy_phys_end);

    serial_println!(
        "Buddy metadata needs ~{} KiB",
        needed_heap / 1024
    );

    let heap_phys_base = phys_base;
    let heap_virt_base = heap_phys_base + hhdm_offset as usize;

    unsafe {
        kheap::ALLOCATOR.lock().init(
            heap_virt_base as *mut u8,
            needed_heap,
        );
    }

    let buddy_data_start = heap_phys_base + needed_heap;

    BUDDY.call_once(|| {
        let mut b = Buddy::empty();
        b.init(buddy_data_start, phys_end);
        Mutex::new(b)
    });

    let pfn = BUDDY
        .get()
        .unwrap()
        .lock()
        .alloc_pfn()
        .expect("Buddy alloc failed");

    serial_println!(
        "Allocated page: phys={:#018x}",
        pfn_to_physical(pfn)
    );

    serial_println!("Physical memory manager initialized!");
}