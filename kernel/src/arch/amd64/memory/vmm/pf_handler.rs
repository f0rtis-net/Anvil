use x86_64::{VirtAddr, registers::control::{Cr2, Cr3}};

use crate::{arch::amd64::{cpu::hlt_loop, memory::{pmm::pages_allocator::{PAllocFlags, alloc_pages_by_order}, vmm::{PAGE_SIZE, map_single_page}}, scheduler::{PerCpuSchedulerData, addr_space::{MapFlags, VmaBacking}, task_storage::get_task_by_index}}, early_println, isr};


isr!(14, page_fault, |frame| {
    let fault_addr = Cr2::read().unwrap();
    let error = frame.error;

    let is_present  = error & (1 << 0) != 0;
    let is_write    = error & (1 << 1) != 0;
    let is_user     = error & (1 << 2) != 0;

    if !is_user {
        early_println!("Kernel page fault at {:#x} error={:#x}", fault_addr.as_u64(), error);
        early_println!("{}", frame);
        hlt_loop();
    }

    let curr_task_id = PerCpuSchedulerData::get().curr_task_id.id();
    let task = get_task_by_index(curr_task_id).unwrap();
    let mut addr_space = task.addr_space.lock();

    match addr_space.find(fault_addr) {
        Some(vma) => {
            if is_write && !vma.flags.contains(MapFlags::WRITE) {
                early_println!("PF: write to read-only VMA at {:#x}", fault_addr.as_u64());
                drop(addr_space);
                hlt_loop();
            }

            match vma.backing {
                VmaBacking::Reserved => {
                    let phys = alloc_pages_by_order(0, PAllocFlags::ZEROED | PAllocFlags::KERNEL)
                        .expect("PF: OOM");

                    let page_vaddr = VirtAddr::new(
                        fault_addr.as_u64() & !(PAGE_SIZE as u64 - 1)
                    );

                    let pt_flags = vma.flags.to_page_table_flags();

                    map_single_page(&mut addr_space.page_table, page_vaddr, phys, pt_flags)
                        .expect("PF: map_single_page failed");

                }
                VmaBacking::Physical { .. } | VmaBacking::Device { .. } => {
                    early_println!("PF: physical VMA not mapped at {:#x}", fault_addr.as_u64());
                    drop(addr_space);
                    hlt_loop();
                    //kill_current_task();
                }
            }
        }
        None => {
            early_println!(
                "PF: segfault at {:#x} (no VMA) task={}",
                fault_addr.as_u64(),
                curr_task_id
            );
            drop(addr_space);
            hlt_loop();
            //kill_current_task();
        }
    }
});