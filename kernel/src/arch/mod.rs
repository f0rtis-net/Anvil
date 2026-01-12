#[cfg(target_arch = "x86_64")]
pub mod amd64;

#[cfg(target_arch = "x86_64")]
pub use amd64 as current;

pub fn hlt_loop() -> ! {
    current::hlt_loop();
}

pub fn arch_init() {
    current::init_arch();
}