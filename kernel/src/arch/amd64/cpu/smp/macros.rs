pub const MSR_GS_BASE: u32 = 0xC000_0101;        
pub const MSR_KERNEL_GS_BASE: u32 = 0xC000_0102; 

#[inline(always)]
pub unsafe fn rdmsr(msr: u32) -> u64 {
    unsafe {
        let lo: u32;
        let hi: u32;
        core::arch::asm!(
            "rdmsr",
            in("ecx") msr,
            out("eax") lo,
            out("edx") hi,
            options(nostack, preserves_flags),
        );
        ((hi as u64) << 32) | (lo as u64)
    }
}

#[inline(always)]
pub fn gs_base_read_u64() -> u64 {
    unsafe { rdmsr(MSR_GS_BASE) }
}

#[inline(always)]
pub fn percpu_ptr<T>(sym: *const T) -> *mut T {
    let gs = gs_base_read_u64();
    let vma = sym as u64;
    (gs.wrapping_add(vma)) as *mut T
}

#[macro_export]
macro_rules! __raw_define_per_cpu {
    (
        section = $section:literal,
        $(#[$attr:meta])*
        $vis:vis $name:ident : $ty:ty,
        incdec = $incdec:literal
    ) => {
        #[unsafe(link_section = $section)]
        $(#[$attr])*
        $vis static $name: core::mem::MaybeUninit<$ty> =
            core::mem::MaybeUninit::zeroed();

        ::paste::paste! {
            #[inline(always)]
            $vis fn [<get_per_cpu_no_guard_ $name>]() -> $ty {
                let sym = core::ptr::addr_of!($name) as *const $ty;
                let ptr = $crate::arch::amd64::cpu::smp::macros::percpu_ptr(sym);
                unsafe { core::ptr::read(ptr) }
            }
        }

        ::paste::paste! {
            #[inline(always)]
            $vis fn [<set_per_cpu_ $name>](v: $ty) {
                let sym = core::ptr::addr_of!($name) as *const $ty;
                let ptr = $crate::arch::amd64::cpu::smp::macros::percpu_ptr(sym);
                unsafe { core::ptr::write(ptr, v) }
            }
        }

        ::paste::paste! {
            #[inline(always)]
            $vis fn [<inc_per_cpu_ $name>]() {
                let sym = core::ptr::addr_of!($name) as *const $ty;
                let ptr = $crate::arch::amd64::cpu::smp::macros::percpu_ptr(sym);
                unsafe {
                    core::arch::asm!(
                        concat!("lock inc ", $incdec, " ptr [{0}]"),
                        in(reg) ptr,
                        options(nostack, preserves_flags),
                    );
                }
            }
        }

        ::paste::paste! {
            #[inline(always)]
            $vis fn [<dec_per_cpu_ $name>]() {
                let sym = core::ptr::addr_of!($name) as *const $ty;
                let ptr = $crate::arch::amd64::cpu::smp::macros::percpu_ptr(sym);
                unsafe {
                    core::arch::asm!(
                        concat!("lock dec ", $incdec, " ptr [{0}]"),
                        in(reg) ptr,
                        options(nostack, preserves_flags),
                    );
                }
            }
        }
    };
}


#[macro_export]
macro_rules! define_per_cpu_u8 {
    ($(#[$attr:meta])* $vis:vis $name:ident) => {
        #[allow(non_snake_case)]
        $crate::__raw_define_per_cpu!(
            section = ".percpu.bss",
            $(#[$attr])*
            $vis $name : u8,
            incdec = "byte"
        );
    };
}

#[macro_export]
macro_rules! define_per_cpu_u16 {
    ($(#[$attr:meta])* $vis:vis $name:ident) => {
        #[allow(non_snake_case)]
        $crate::__raw_define_per_cpu!(
            section = ".percpu.bss",
            $(#[$attr])*
            $vis $name : u16,
            incdec = "word"
        );
    };
}

#[macro_export]
macro_rules! define_per_cpu_u32 {
    ($(#[$attr:meta])* $vis:vis $name:ident) => {
        #[allow(non_snake_case)]
        $crate::__raw_define_per_cpu!(
            section = ".percpu.bss",
            $(#[$attr])*
            $vis $name : u32,
            incdec = "dword"
        );
    };
}

#[macro_export]
macro_rules! define_per_cpu_u64 {
    ($(#[$attr:meta])* $vis:vis $name:ident) => {
        #[allow(non_snake_case)]
        $crate::__raw_define_per_cpu!(
            section = ".percpu.bss",
            $(#[$attr])*
            $vis $name : u64,
            incdec = "qword"
        );
    };
}

#[macro_export]
macro_rules! define_per_cpu_struct {
    (
        $(#[$attr:meta])*
        $vis:vis struct $name:ident {
            $(
                $field_vis:vis $field:ident : $ty:ty
            ),* $(,)?
        }
    ) => {
        #[repr(C, align(64))]
        $(#[$attr])*
        $vis struct $name {
            $( $field_vis $field : $ty ),*
        }

        #[unsafe(link_section = ".percpu.bss")]
        static $name: core::mem::MaybeUninit<$name> =
            core::mem::MaybeUninit::zeroed();

        impl $name {
            #[inline(always)]
            fn __gs_base_u64() -> u64 {
                $crate::arch::amd64::cpu::smp::macros::gs_base_read_u64()
            }

            #[inline(always)]
            fn __sym_vma_u64() -> u64 {
                core::ptr::addr_of!($name) as u64
            }

            #[inline(always)]
            pub fn get() -> &'static Self {
                unsafe {
                    let ptr_u64 = Self::__gs_base_u64().wrapping_add(Self::__sym_vma_u64());
                    &*(ptr_u64 as *const $name)
                }
            }

            #[inline(always)]
            pub fn get_mut() -> &'static mut Self {
                let ptr_u64 = Self::__gs_base_u64().wrapping_add(Self::__sym_vma_u64());
                unsafe { &mut *(ptr_u64 as *mut $name) }
            }

            #[inline(always)]
            pub fn with_guard<R>(f: impl FnOnce(&mut Self) -> R) -> R {
                let _guard =
                    $crate::arch::amd64::cpu::smp::preempt::PreemptGuard::new(());
                f(Self::get_mut())
            }
        }
    };
}
