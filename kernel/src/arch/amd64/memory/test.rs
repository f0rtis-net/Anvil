use x86_64::{VirtAddr, structures::paging::PageTableFlags};

use crate::{arch::amd64::memory::{pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order, free_pages}, vmm::{map_page, unmap_page}}, serial_println};

pub fn test_all_memory_subsystem() {
    serial_println!("\n ===== Memory subsystem fast test =====");

    let phys = alloc_pages_by_order(
        0,
        PAllocFlags::Kernel | PAllocFlags::Zeroed
    ).expect("alloc_pages_by_order failed");

    serial_println!("[mem] allocated phys frame: {:?}", phys);

    let virt = VirtAddr::new(0x10000);
    serial_println!("[mem] mapping virt: {:?}", virt);

    map_page(
        virt,
        phys,
        PageTableFlags::PRESENT | PageTableFlags::WRITABLE
    );

    unsafe {
        let p = virt.as_ptr::<u64>();
        serial_println!("[mem] initial value: {:#x}", *p);
        assert!(*p == 0);
    }

    unsafe {
        let p = virt.as_ptr::<u64>() as *mut u64;
        *p = 0xdead_c0de_dead_c0de;
    }

    unsafe {
        let p = virt.as_ptr::<u64>();
        serial_println!("[mem] readback value: {:#x}", *p);
        assert!(*p == 0xdead_c0de_dead_c0de);
    }

    unmap_page(virt);
    serial_println!("[mem] unmapped virt: {:?}", virt);

    free_pages(phys);
    serial_println!("[mem] freed phys frame");

    serial_println!(" ===== Memory subsystem fast test =====\n");
}