use core::{fmt, marker::PhantomData, ptr::{read_volatile, write_volatile}};
use core::fmt::Debug;

#[derive(Clone, Copy)]
pub struct RegisterRW<T> {
    address: usize,
    _phantom: PhantomData<*mut T>, // важно: отражаем mutability
}

impl<T> RegisterRW<T> {
    /// # Safety
    /// Caller must guarantee that `address` is valid for `T`
    /// and properly aligned.
    pub const unsafe fn from_address(address: usize) -> Self {
        Self {
            address,
            _phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn ptr(&self) -> *mut T {
        self.address as *mut T
    }

    #[inline(always)]
    pub fn read(&self) -> T {
        unsafe { read_volatile(self.ptr()) }
    }

    #[inline(always)]
    pub fn write(&self, val: T) {
        unsafe { write_volatile(self.ptr(), val) }
    }

    #[inline(always)]
    pub fn modify(&self, f: impl FnOnce(T) -> T) {
        let val = self.read();
        self.write(f(val));
    }

    #[inline(always)]
    pub fn modify_mut(&self, f: impl FnOnce(&mut T)) {
        let mut val = self.read();
        f(&mut val);
        self.write(val);
    }
}

impl<T: Debug> Debug for RegisterRW<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        debug_fmt_register("RegisterRW", ValOrStr::Val(self.read()), self.address, f)
    }
}

#[derive(Clone, Copy)]
pub struct RegisterRO<T> {
    address: usize,
    _phantom: PhantomData<*const T>,
}

impl<T> RegisterRO<T> {
    /// # Safety
    /// Caller must guarantee correctness of address and alignment.
    pub const unsafe fn from_address(address: usize) -> Self {
        Self {
            address,
            _phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn ptr(&self) -> *const T {
        self.address as *const T
    }

    #[inline(always)]
    pub fn read(&self) -> T {
        unsafe { read_volatile(self.ptr()) }
    }
}

impl<T: Debug> Debug for RegisterRO<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        debug_fmt_register("RegisterRO", ValOrStr::Val(self.read()), self.address, f)
    }
}

#[derive(Clone, Copy)]
pub struct RegisterROSideEffect<T> {
    address: usize,
    _phantom: PhantomData<*const T>,
}

impl<T: Copy> RegisterROSideEffect<T> {
    /// # Safety
    /// Caller must guarantee:
    /// - address is valid
    /// - properly aligned for T
    /// - mapped into virtual space
    pub const unsafe fn from_address(address: usize) -> Self {
        Self {
            address,
            _phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn ptr(&self) -> *const T {
        self.address as *const T
    }

    #[inline(always)]
    pub fn read(&self) -> T {
        unsafe { read_volatile(self.ptr()) }
    }
}


#[derive(Clone, Copy, Debug)]
pub struct RegisterWO<T> {
    address: usize,
    _phantom: PhantomData<*mut T>,
}

impl<T: Copy> RegisterWO<T> {
    /// # Safety
    /// Caller must guarantee:
    /// - address is valid for T
    /// - properly aligned
    /// - mapped into virtual memory
    pub const unsafe fn from_address(address: usize) -> Self {
        Self {
            address,
            _phantom: PhantomData,
        }
    }

    #[inline(always)]
    fn ptr(&self) -> *mut T {
        self.address as *mut T
    }

    #[inline(always)]
    pub fn write(&self, val: T) {
        unsafe {
            write_volatile(self.ptr(), val);
        }
    }
}

enum ValOrStr<T> {
    Val(T),
    Str(&'static str),
}

#[macro_export]
macro_rules! register_struct {
    (
        $(#[$attr:meta])*
        $vis:vis $struct_name:ident {
            $(
                $offset:literal => $name:ident : $register_type:ident $(< $type:ty >)?
            ),* $(,)?
        }
    ) => {
        $(#[$attr])*
        #[derive(Clone, Copy)]
        $vis struct $struct_name {
            address: usize,
        }

        impl $struct_name {
            $vis unsafe fn from_address(address: usize) -> Self {
                Self { address }
            }

            $(
                $crate::register_struct!(@register_method $vis, $offset, $name, $register_type, $(< $type >)? );
            )*
        }

        impl core::fmt::Debug for $struct_name {
            fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
                f.debug_struct(stringify!($struct_name))
                    .field("address", &self.address)
                    $(
                        .field(stringify!($name), &self.$name())
                    )*
                    .finish()
            }
        }
    };

    (@register_method $vis:vis, $offset:expr, $name:ident, $register_type:ident, $(< $type:ty >)? ) => {
        $vis fn $name(&self) -> $register_type $(< $type >)? {
            unsafe { $register_type::from_address(self.address + $offset as usize) }
        }
    };
}

fn debug_fmt_register<T: Debug>(
    struct_name: &str,
    value: ValOrStr<T>,
    address: usize,
    f: &mut fmt::Formatter<'_>,
) -> fmt::Result {
    f.write_str(struct_name)?;
    f.write_str("(")?;

    match value {
        ValOrStr::Val(value) => value.fmt(f)?,
        ValOrStr::Str(s) => f.write_str(s)?,
    }

    f.write_str(" [")?;

    let ptr = address as *const T;
    write!(f, "{:p}", ptr)?;

    f.write_str("])")
}