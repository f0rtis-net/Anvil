use core::arch::asm;
use core::marker::PhantomData;

pub trait InOut {
    fn port_in(port: u16) -> Self;
    fn port_out(port: u16, val: Self);
}

impl InOut for u8 {
    fn port_in(port: u16) -> Self {
        let mut val;
        unsafe { asm!("in al, dx", out("al") val, in("dx") port); }
        return val;
    }

    fn port_out(port: u16, val: Self) {
        unsafe { asm!("out dx, al", in("al") val, in("dx") port); }
    }
}

impl InOut for u16 {
    fn port_in(port: u16) -> Self {
        let mut val;
        unsafe { asm!("in ax, dx", out("ax") val, in("dx") port); }
        return val;
    }

    fn port_out(port: u16, val: Self) {
        unsafe { asm!("out dx, ax", in("ax") val, in("dx") port); }
    }
}

impl InOut for u32 {
    fn port_in(port: u16) -> Self {
        let mut val;
        unsafe { asm!("in eax, dx", out("eax") val, in("dx") port); }
        return val;
    }

    fn port_out(port: u16, val: Self) {
        unsafe { asm!("out dx, eax", in("eax") val, in("dx") port); }
    }
}

pub struct Port<T>
where
    T: InOut,
{
    port: u16,
    pt: PhantomData<T>,
}

impl<T> Port<T>
where
    T: InOut,
{
    pub const fn new(port: u16) -> Port<T> {
        Port {
            port,
            pt: PhantomData,
        }
    }

    pub fn write(&self, val: T) {
        T::port_out(self.port, val);  
    }

    pub fn read(&self) -> T {
        T::port_in(self.port)
    }
}