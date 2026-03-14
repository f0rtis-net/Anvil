use core::{arch::asm, sync::atomic::{AtomicU8, Ordering}};

use alloc::boxed::Box;
use limine::{mp::Cpu, response::MpResponse};
use x86_64::instructions;

use crate::{arch::amd64::{apic::init_lapic_percpu, cpu::{hlt_loop, smp::percpu::{PerCpuRegion, init_percpu_regions, set_cpu_id, set_gsbase_for_percpu_region}}, gdt::setup_gdt_for_local_core, interrupts::idt::init_idt, scheduler::{global_init_scheduler, init_scheduler_percpu}}, bootinfo::BootInfo, define_per_cpu_u32, early_println, isr};

static NUM_CPUS_BOOTSTRAPPED: AtomicU8 = AtomicU8::new(0);

pub(crate) struct LimineCPU {
    pub(crate) mp_response: &'static MpResponse,
    pub(crate) cpu: &'static Cpu,
}

impl LimineCPU {
    pub(crate) fn bootstrap_cpu(
        &self,
        entry: unsafe extern "C" fn(&Cpu) -> !,
        region: &'static PerCpuRegion,
    ) {
        #[cfg(target_arch = "x86_64")]
        if self.mp_response.bsp_lapic_id() == self.cpu.lapic_id {
            return;
        }

        let ptr = region as *const PerCpuRegion as u64;
        self.cpu.extra.store(ptr, Ordering::Release);

        self.cpu.goto_address.write(entry);
    }

    pub (crate) fn bootstrap_bsp_cpu(
        &self,
        entry: unsafe extern "C" fn(&Cpu) -> !,
        region: &'static PerCpuRegion
    ) {
        let ptr = region as *const PerCpuRegion as u64;
        self.cpu.extra.store(ptr, Ordering::Release);

        self.cpu.goto_address.write(entry);
    }
}

struct CPUIterator {
    mp_response: &'static MpResponse,
    current: usize,
}

impl Iterator for CPUIterator {
    type Item = LimineCPU;

    fn next(&mut self) -> Option<Self::Item> {
        let cpu = self.mp_response.cpus().get(self.current)?;
        self.current += 1;

        Some(LimineCPU {
            mp_response: self.mp_response,
            cpu: *cpu,
        })
    }
}

fn get_smp_entries() -> impl Iterator<Item = LimineCPU> {
    let mp_response = BootInfo::get().get_smp_response()
        .expect("failed to get limine SMP response");

    CPUIterator {
        mp_response,
        current: 0,
    }
}

#[unsafe(no_mangle)]
unsafe extern "C" fn start_ap(info: &Cpu) -> ! {
    instructions::interrupts::disable();
    let region_ptr = info.extra.load(Ordering::Acquire) as *const PerCpuRegion;
    assert!(!region_ptr.is_null());
    let local_region = unsafe { &*region_ptr };
    set_gsbase_for_percpu_region(local_region.base);
    set_cpu_id(info.lapic_id);
    setup_gdt_for_local_core();
    init_idt();
    init_lapic_percpu();
    NUM_CPUS_BOOTSTRAPPED.fetch_add(1, Ordering::Release);
    instructions::interrupts::enable();
    init_scheduler_percpu();
}

pub fn smp_startup() {
    let regions = init_percpu_regions();
    let regions: &'static [PerCpuRegion] = Box::leak(regions.into_boxed_slice());

    early_println!("All cpus count: {}", regions.len());
    global_init_scheduler(regions.len());

    for (i, entry) in get_smp_entries().enumerate() {
        entry.bootstrap_cpu(start_ap, &regions[i]);
    }

    early_println!("Waiting for slave cpus...");
    while NUM_CPUS_BOOTSTRAPPED.load(Ordering::Acquire) < (regions.len() - 1) as u8 {
        core::hint::spin_loop();
    }

    early_println!("All slave cpus initialized!");
}
