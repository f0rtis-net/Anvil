use lazy_static::lazy_static;
use x86_64::{PrivilegeLevel, VirtAddr, instructions::tables::load_tss, registers::segmentation::{CS, DS, ES, SS, Segment}, structures::{gdt::{Descriptor, GlobalDescriptorTable, SegmentSelector}, tss::TaskStateSegment}};


lazy_static! {
    pub static ref TSS: TaskStateSegment = {
        let mut tss = TaskStateSegment::new();

        tss.privilege_stack_table[0] = {
            const STACK_SIZE: usize = 4096 * 2;
            static mut STACK: [u8; STACK_SIZE] = [0; STACK_SIZE];

            let stack_start = VirtAddr::from_ptr(&raw const STACK);
            let stack_end = stack_start + STACK_SIZE as u64;
            stack_end
        };
        tss
    };
}

struct Selectors {
    kcode_selector: SegmentSelector,
    kdata_selector: SegmentSelector,
    ucode_selector: SegmentSelector,
    udata_selector: SegmentSelector,
    tss_selector: SegmentSelector,
}

lazy_static! {
    static ref GDT: (GlobalDescriptorTable, Selectors) = {
        let mut gdt = GlobalDescriptorTable::new();

        let kcode_selector = gdt.append(Descriptor::kernel_code_segment());
        let kdata_selector = gdt.append(Descriptor::kernel_data_segment());

        let ucode_selector = gdt.append(Descriptor::user_code_segment());
        let udata_selector = gdt.append(Descriptor::user_data_segment());

        let tss_selector = gdt.append(Descriptor::tss_segment(&TSS));
        (gdt, Selectors { kcode_selector, kdata_selector, ucode_selector, udata_selector, tss_selector })
    };
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