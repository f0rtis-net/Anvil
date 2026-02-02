use x86_64::instructions::hlt;

pub mod frames;
pub mod cpuid;
pub mod smp;

pub fn hlt_loop() -> !{
    loop {
        hlt();
    }
}



