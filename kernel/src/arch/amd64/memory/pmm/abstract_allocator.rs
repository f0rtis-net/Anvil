use crate::{arch::amd64::memory::{
    misc::align_up,
    pmm::{
        alloc_pages::{alloc_pages, free_pages}, frame_alloc::pages_to_order, frame_area::FRAME_SIZE, mem_zones::MemoryZoneType, slab::{SLAB_MAX_ALLOC, slab_alloc, slab_try_free} 
    },
}, serial_println};

bitflags::bitflags! {
    pub struct KmallocFlags: u32 {
        const Kernel = 1 << 0;
        const Zeroed = 1 << 2;
    }
}

pub fn kmalloc(bytes: usize, flags: KmallocFlags) -> usize {
    if bytes == 0 {
        return 0;
    }

    let zeroed = flags.contains(KmallocFlags::Zeroed);

    if bytes <= SLAB_MAX_ALLOC {
        if let Some(p) = slab_alloc(bytes, zeroed) {
            serial_println!("SLUB ALLOCATOR USED");
            return p;
        }
    }

    let bytes = align_up(bytes, FRAME_SIZE);
    let pages = bytes / FRAME_SIZE;
    let order = pages_to_order(pages);

    let virt = alloc_pages(order, MemoryZoneType::High, zeroed)
        .expect("kmalloc: OOM");

    virt
}


pub fn kfree(ptr: usize) {
    if ptr == 0 {
        return;
    }

    if slab_try_free(ptr) {
        return;
    }

    free_pages(ptr);
}
