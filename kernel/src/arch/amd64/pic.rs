use x86_64::instructions;

use crate::arch::amd64::ports::Port;

const PIC_MASTER_PORT: u16 = 0x20;
const PIC_SLAVE_PORT: u16 = 0xA0;
const WAIT_PORT: u16 = 0x11;

const ICW1_ICW4: u8 = 0x01; // ICW4 (not) needed
const ICW1_INIT: u8 = 0x10; // Initialization - required!
const ICW4_8086: u8 = 0x01; // 8086/88 (MCS-80/85) mode

const PIC_MASTER_NEW_OFFSET: u8 = 0x20;
const PIC_SLAVE_NEW_OFFSET: u8 = 0x28;

const END_OF_INTERRUPT: u8 = 0x20;

pub fn init_pics() {
    let master_cmd: Port<u8> = Port::new(PIC_MASTER_PORT);
    let master_data: Port<u8> = Port::new(PIC_MASTER_PORT + 1);
    let slave_cmd: Port<u8> = Port::new(PIC_SLAVE_PORT);
    let slave_data: Port<u8> = Port::new(PIC_SLAVE_PORT + 1);
    let wait_port: Port<u8> = Port::new(WAIT_PORT);
    let wait = || wait_port.write(0);

    // save interrupt masks
    let a1 = master_data.read();
    let a2 = slave_data.read();

    instructions::interrupts::disable();

    // begin initialization
    master_cmd.write(ICW1_INIT + ICW1_ICW4);
    wait();
    slave_cmd.write(ICW1_INIT + ICW1_ICW4);
    wait();

    // set interrupt offsets
    master_data.write(PIC_MASTER_NEW_OFFSET);
    wait();
    slave_data.write(PIC_SLAVE_NEW_OFFSET);
    wait();

    // chain slave PIC to master
    master_data.write(4); // tell master there is a slave PIC at IRQ2
    wait();
    slave_data.write(2); // tell slave it's cascade
    wait();

    // set mode
    master_data.write(ICW4_8086);
    wait();
    slave_data.write(ICW4_8086);
    wait();

    // restore interrupt masks
    master_data.write(a1);
    slave_data.write(a2);

    instructions::interrupts::enable();
}

pub fn eoi(interrupt_id: u8) {
    if interrupt_id >= PIC_SLAVE_NEW_OFFSET && interrupt_id < PIC_SLAVE_NEW_OFFSET + 8 {
        Port::new(PIC_SLAVE_PORT).write(END_OF_INTERRUPT);
    }
    Port::new(PIC_MASTER_PORT).write(END_OF_INTERRUPT);
}

pub fn setup_timer_freq(freq: usize) {
    const PIT_BASE_FREQ: usize = 1193182;
    const PIT_COMMAND_PORT: u16 = 0x43;
    const PIT_CHANNEL0_PORT: u16 = 0x40;

    if freq == 0 {
        return;
    }

    let divisor: u16 = (PIT_BASE_FREQ / freq) as u16;

    Port::<u8>::new(PIT_COMMAND_PORT).write(0x36);

    let chan0: Port<u8> = Port::new(PIT_CHANNEL0_PORT);
    chan0.write((divisor & 0xFF) as u8);       // low byte
    chan0.write(((divisor >> 8) & 0xFF) as u8); // high byte
}

pub fn disable_pit() {
    const PIT_COMMAND_PORT: u16 = 0x43;
    const PIT_CHANNEL0_PORT: u16 = 0x40;

    Port::<u8>::new(PIT_COMMAND_PORT).write(0x30); // select channel 0
    let chan: Port<u8> = Port::new(PIT_CHANNEL0_PORT);
    chan.write(0);
    chan.write(0); // set freq to 0
}