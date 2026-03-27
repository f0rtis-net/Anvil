use core::u32;

use pc_keyboard::{DecodedKey, HandleControl, Keyboard, ScancodeSet1, layouts};
use spin::{Mutex, Once};
use x86_64::VirtAddr;

use crate::{
    arch::amd64::{
        acpi::{get_acpi_tables, madt::MadTable}, apic::{ioapic::{IOAPICRedirectionTableRegister, IOApic}, lapic::{Lapic, LapicTimerDivide}}, memory::misc::phys_to_virt, ports::Port, timer::get_hpet
    }, define_per_cpu_struct, early_print, early_println, irq
};

pub mod lapic;
pub mod ioapic;

static LAPIC: Once<Lapic> = Once::new();

define_per_cpu_struct! {
    pub struct PercpuLapic {
        pub lapic: Lapic
    }
}

const PIC_MASTER_PORT: u16 = 0x20;
const PIC_SLAVE_PORT: u16 = 0xA0;
const TIMER_VECTOR: u8 = 0x30;

fn disable_pic() {
    let pic1 = Port::<u8>::new(PIC_MASTER_PORT + 1);
    let pic2 = Port::<u8>::new(PIC_SLAVE_PORT + 1);

    pic1.write(0xFF);
    pic2.write(0xFF);
}

pub fn calibrate_lapic_timer(lapic: &Lapic) -> u32 {
    const CALIBRATION_MS: u64 = 10;

    let hpet_ticks_target =
        (CALIBRATION_MS * 1_000_000_000_000u64) / get_hpet().read().period_fs();

    lapic.set_timer_divide(LapicTimerDivide::Div16);
    lapic.set_lvt_timer(TIMER_VECTOR, false);
    lapic.set_timer_initial(u32::MAX);

    let hpet_start = get_hpet().read().read_counter();
    let lapic_start = lapic.read_timer_current();

    while get_hpet().read().read_counter().wrapping_sub(hpet_start) < hpet_ticks_target {
        core::hint::spin_loop();
    }

    let lapic_end = lapic.read_timer_current();

    lapic_start.wrapping_sub(lapic_end)
}

pub fn init_lapic_percpu() {
    let lapic_addr = get_acpi_tables().read().get_table::<MadTable>().unwrap().lapic_addr;
    PercpuLapic::with_guard(|plapic| {
        let lapic_virt = VirtAddr::new(phys_to_virt(lapic_addr.as_u64() as usize) as u64);
        plapic.lapic = Lapic::new(lapic_addr, lapic_virt);
        plapic.lapic.enable();
        plapic.lapic.set_task_priority(0);
    });
}

pub fn init_bootstrap_lapic() {
    early_println!("Disabling legacy PIC...");
    disable_pic();
    early_println!("Legacy PIC disabled");
    let lapic_addr = get_acpi_tables().read().get_table::<MadTable>().unwrap().lapic_addr;
    LAPIC.call_once(|| {
        let lapic_virt = VirtAddr::new(phys_to_virt(lapic_addr.as_u64() as usize) as u64);
        let lapic = Lapic::new(lapic_addr, lapic_virt);
        early_println!("LAPIC ID: {}", lapic.id());
        lapic.enable();
        lapic.set_task_priority(0);
        early_println!("Lapic enabled!");
        lapic
    });
}

pub fn start_timer(lapic: &Lapic) {
    let ticks_10ms = calibrate_lapic_timer(lapic);
    let ticks_1ms = ticks_10ms / 10;

    lapic.setup_timer_periodic(
        TIMER_VECTOR,
        LapicTimerDivide::Div16,          
        ticks_1ms,  
    );
}

static IOAPIC: Once<IOApic> = Once::new();

pub fn init_ioapic() {
    IOAPIC.call_once(|| {
        IOApic::new()
    });

    KEYBOARD.lock().replace(Keyboard::new(
        ScancodeSet1::new(),
        layouts::Us104Key,
        HandleControl::Ignore
    ));

    install_ioapic_irq(1, 150);
}

pub fn install_ioapic_irq(irq_num: u8, vector_num: u8) {
    let ioapic = IOAPIC.get().expect("IOAPIC is not initialized!");
    ioapic.write_ioredtbl(irq_num, IOAPICRedirectionTableRegister::new()
        .with_interrupt_vector(vector_num)
        .with_interrupt_mask(false)
        .with_delivery_mode(0)
        .with_destination_mode(false)
        .with_delivery_status(false)
        .with_destination_field(ioapic.ioapic_id().id()));
}

pub fn lapic_eoi() {
    let gsbase: u64;
    unsafe {
        core::arch::asm!(
            "rdgsbase {}", 
            out(reg) gsbase,
            options(nostack, nomem)
        );
    }
    if gsbase != 0 {
        PercpuLapic::with_guard(|p| p.lapic.eoi());
    } else {
        LAPIC.get().unwrap().eoi();
    }
}

static KEYBOARD: Mutex<Option<Keyboard<layouts::Us104Key, ScancodeSet1>>> = Mutex::new(None);

irq!(150, keyboard_irq, |stack| {
    const KEYBOARD_PORT: u16 = 0x60;

    let mut lock = KEYBOARD.lock();
    let keyboard = lock.as_mut().expect("keyboard not initialized");
    let port = Port::<u8>::new(KEYBOARD_PORT);

    let scancode: u8 = port.read();

    if let Ok(Some(key_event)) = keyboard.add_byte(scancode) {
        if let Some(key) = keyboard.process_keyevent(key_event) {
            match key {
                DecodedKey::Unicode(character) => {
                    early_print!("{character}");
                }
                _ => () //temporally unhandled 
            }
        }
    }

    lapic_eoi();
});
