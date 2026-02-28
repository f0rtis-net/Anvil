use alloc::{collections::btree_map::BTreeMap, vec::Vec};
use spin::Mutex;
use x86_64::{PhysAddr, VirtAddr, structures::paging::PageTableFlags};

use crate::arch::amd64::memory::{misc::align_up, pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order, free_pages}, vmm::{PAGE_SIZE, kmap_page, kunmap_page}};

struct VmallocRegion {
    base: VirtAddr,
    size: usize,
    pages: Vec<PhysAddr>
}

static VMALLOC: Mutex<VmallocManager> = Mutex::new(
    VmallocManager {
        regions: BTreeMap::new(),
    }
);


struct VmallocManager {
    regions: BTreeMap<VirtAddr, VmallocRegion>
}

const VMALLOC_START: VirtAddr = VirtAddr::new(0xffff_ffff_c000_0000);
const VMALLOC_END:   VirtAddr = VirtAddr::new(0xffff_ffff_f000_0000);

fn find_free_range(vm: &VmallocManager, size: usize) -> Option<VirtAddr> {
    let mut cursor = VMALLOC_START;

    for region in vm.regions.values() {
        if cursor.as_u64() + size as u64 <= region.base.as_u64() {
            return Some(cursor);
        }

        let region_end =
            region.base.as_u64() + region.size as u64;

        cursor = VirtAddr::new(region_end);
    }

    if cursor.as_u64() + size as u64 <= VMALLOC_END.as_u64() {
        return Some(cursor);
    }

    None
}


pub fn vmalloc(size: usize) -> Option<VirtAddr> {
    let size = align_up(size, PAGE_SIZE);
    let mut vm = VMALLOC.lock();

    let base = find_free_range(&vm, size)?;

    let mut pages = Vec::new();

    for off in (0..size).step_by(PAGE_SIZE) {
        let phys = alloc_pages_by_order(0, PAllocFlags::ZEROED | PAllocFlags::KERNEL)?;
        kmap_page(base + off as u64, phys, PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE | PageTableFlags::NO_CACHE);
        pages.push(phys);
    }

    vm.regions.insert(
        base,
        VmallocRegion {
            base,
            size,
            pages,
        },
    );

    Some(base)
}

pub fn vfree(ptr: VirtAddr) {
    let mut vm = VMALLOC.lock();

    let region = vm.regions.remove(&ptr)
        .expect("vfree: invalid pointer");

    for (i, phys) in region.pages.iter().enumerate() {
        let virt = region.base + (i * PAGE_SIZE) as u64;
        kunmap_page(virt);
        free_pages(*phys);
    }
}