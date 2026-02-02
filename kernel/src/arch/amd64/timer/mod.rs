use core::ptr::{NonNull, read_volatile, write_volatile};

use spin::{Once, RwLock};

use crate::{arch::amd64::{
    acpi::{get_acpi_tables, hpet::HpetTableParsed},
    memory::vmm::map_mmio_region,
}, early_println};

const HPET_CFG_ENABLE: u64 = 1 << 0;
const HPET_CFG_LEGACY: u64 = 1 << 1;

#[inline(always)]
fn mmio_read<T>(ptr: *const T) -> T {
    unsafe { read_volatile(ptr) }
}

#[inline(always)]
fn mmio_write<T>(ptr: *mut T, val: T) {
    unsafe { write_volatile(ptr, val) }
}

#[repr(C)]
struct HpetRegisters {
    general_cap_id:     u64,      // 0x000
    _rsv0:              u64,      // 0x008
    general_config:     u64,      // 0x010
    _rsv_cfg2:          u64,      // 0x018 (RESERVED)
    general_int_status: u64,      // 0x020
    _rsv1:              [u64; 25],// 0x028 .. 0x0EF
    main_counter:       u64,      // 0x0F0
}

static HPET_GLOBAL: Once<RwLock<HPET>> = Once::new();

#[inline]
pub fn get_hpet() -> &'static RwLock<HPET>{
    HPET_GLOBAL
        .get()
        .expect("HPET not inited yet")
}

pub struct HPET {
    regs: NonNull<HpetRegisters>,
    /// femtoseconds per tick (from capabilities bits 63:32)
    period_fs: u64,
}

unsafe impl Send for HPET {}
unsafe impl Sync for HPET {}

impl HPET {
    pub fn init(&mut self, enable_legacy: bool) {
        unsafe {
            let r = self.regs.as_ptr();

            // Disable
            mmio_write(&mut (*r).general_config, 0);

            // Read caps (period in fs in bits 63:32)
            let caps = mmio_read(&(*r).general_cap_id);
            self.period_fs = caps >> 32;

            // Reset main counter
            mmio_write(&mut (*r).main_counter, 0);

            // Enable (+ optional legacy replacement)
            let mut cfg = HPET_CFG_ENABLE;
            if enable_legacy {
                cfg |= HPET_CFG_LEGACY;
            }
            mmio_write(&mut (*r).general_config, cfg);
        }
    }

    #[inline(always)]
    pub fn read_counter(&self) -> u64 {
        unsafe { mmio_read(&(*self.regs.as_ptr()).main_counter) }
    }

    #[inline(always)]
    pub fn period_fs(&self) -> u64 {
        self.period_fs
    }

    pub fn is_ticking(&self, spins: u32) -> bool {
        let a = self.read_counter();
        let mut b = a;

        for _ in 0..spins {
            core::hint::spin_loop();
            b = self.read_counter();
            if b != a {
                return true;
            }
        }
        false
    }
}


pub fn initialize_hpet() {
    let hpet_phys_base = {
        let guard = get_acpi_tables().read();
        guard
            .get_table::<HpetTableParsed>()
            .expect("HPET table not present")
            .base_address
    };

    let hpet_virt = map_mmio_region(hpet_phys_base, core::mem::size_of::<HpetRegisters>());

    let regs = NonNull::new(hpet_virt.as_mut_ptr::<HpetRegisters>())
        .expect("HPET MMIO mapping failed");

    let mut hpet = HPET {
        regs,
        period_fs: 0,
    };

    hpet.init(true);

    let ticking = hpet.is_ticking(50_000_000);

    if !ticking {
        early_println!("HPET is not ticking! Fallback to lapic timer...");
    }

    HPET_GLOBAL.call_once(|| {
        RwLock::new(hpet)
    });
}

