#[cfg(target_arch = "x86_64")]
pub mod amd64;

#[cfg(target_arch = "x86_64")]
pub use amd64 as current;
use limine::memory_map::Entry;

pub fn hlt_loop() -> ! {
    current::hlt_loop();
}

pub struct ArchInitInfo<'a> {
    pub hhdm_offset: u64,
    pub memmap_entry: &'a[&'a Entry]
}

pub fn arch_init(arch_init: ArchInitInfo) {
    current::init_arch(arch_init);
}