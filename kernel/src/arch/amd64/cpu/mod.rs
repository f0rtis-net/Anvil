use x86_64::instructions::hlt;

pub mod frames;

pub fn hlt_loop() -> !{
    loop {
        hlt();
    }
}