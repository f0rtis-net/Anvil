pub const DOUBLE_FAULT_IST_INDEX: u16 = 0;

use lazy_static::lazy_static;
use x86_64::{registers::segmentation::{DS, ES, SS}, structures::{gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, tss::TaskStateSegment}, PrivilegeLevel, VirtAddr};
use x86_64::instructions::tables::load_tss;
use x86_64::instructions::segmentation::{CS, Segment};

lazy_static! {
    pub static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();
        tss.interrupt_stack_table[DOUBLE_FAULT_IST_INDEX as usize] = {
            const STACK_SIZE: usize = 4096 * 5;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };

        tss.privilege_stack_table[0] = {
            const STACK_SIZE: usize = 4096 * 2;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            let stack_end = stack_start + STACK_SIZE;
            stack_end
        };
        tss
    };
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let kcode_selector = gdt.add_entry(Descriptor::kernel_code_segment());
        let kdata_selector = gdt.add_entry(Descriptor::kernel_data_segment());

        let ucode_selector = gdt.add_entry(Descriptor::user_code_segment());
        let udata_selector = gdt.add_entry(Descriptor::user_data_segment());

        let tss_selector = gdt.add_entry(Descriptor::tss_segment(&TSS));
        (gdt, Selectors { kcode_selector, kdata_selector, ucode_selector, udata_selector, tss_selector })
    };
}

struct Selectors {
    kcode_selector: SegmentSelector,
    kdata_selector: SegmentSelector,
    ucode_selector: SegmentSelector,
    udata_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

pub fn init_gdt() {
    GDT.0.load();
    unsafe {
        CS::set_reg(GDT.1.kcode_selector);

        DS::set_reg(GDT.1.kdata_selector);
        ES::set_reg(GDT.1.kdata_selector);
        SS::set_reg(GDT.1.kdata_selector);  
        
        load_tss(GDT.1.tss_selector);
    }
}

pub fn kcs_sel() -> SegmentSelector {
    SegmentSelector::new(GDT.1.kcode_selector.index(), PrivilegeLevel::Ring0)
}

pub fn kds_sel() -> SegmentSelector {
    SegmentSelector::new(GDT.1.kdata_selector.index(), PrivilegeLevel::Ring0)
}

pub fn ucs_sel() -> SegmentSelector {
    SegmentSelector::new(GDT.1.ucode_selector.index(), PrivilegeLevel::Ring3)
}

pub fn uds_sel() -> SegmentSelector {
    SegmentSelector::new(GDT.1.udata_selector.index(), PrivilegeLevel::Ring3)
}