use x86_64::{VirtAddr, structures::paging::PageTableFlags};
use crate::{arch::amd64::memory::{pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order, free_pages}, vmm::{kmap_page, kunmap_page}}, early_println};

pub fn selftest_all_memory_subsystem() {
    early_println!("\n ===== Memory subsystem fast test =====");

    let phys = alloc_pages_by_order(
        0,
        PAllocFlags::KERNEL | PAllocFlags::ZEROED
    ).expect("alloc_pages_by_order failed");

    early_println!("[mem] allocated phys frame: {:?}", phys);

    let virt = VirtAddr::new(0x10000);
    early_println!("[mem] mapping virt: {:?}", virt);

    kmap_page(
        virt,
        phys,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE
    );

    unsafe {
        let p = virt.as_ptr::<u64>();
        early_println!("[mem] initial value: {:#x}", *p);
        assert!(*p == 0);
    }

    unsafe {
        let p = virt.as_ptr::<u64>() as *mut u64;
        *p = 0xdead_c0de_dead_c0de;
    }

    unsafe {
        let p = virt.as_ptr::<u64>();
        early_println!("[mem] readback value: {:#x}", *p);
        assert!(*p == 0xdead_c0de_dead_c0de);
    }

    kunmap_page(virt);
    early_println!("[mem] unmapped virt: {:?}", virt);

    free_pages(phys);
    early_println!("[mem] freed phys frame");

    early_println!(" ===== Memory subsystem fast test =====");
}