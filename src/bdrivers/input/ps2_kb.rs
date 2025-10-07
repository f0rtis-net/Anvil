use alloc::sync::Arc;
use pc_keyboard::{layouts, Keyboard, KeyboardLayout, ScancodeSet1};
use spin::RwLock;
use x86_64::instructions::port::Port;

use crate::{bdrivers::input::{
    events::{InputEvent, InputEventType},
    input_register_device, input_report_event, InputDevice,
}, println};

lazy_static::lazy_static! {
    static ref KBD_DEVICE: RwLock<Option<Arc<InputDevice>>> = RwLock::new(None);
    static ref KBD_PORT: RwLock<Port<u8>> = RwLock::new(unsafe { Port::new(0x60) });
}

pub fn handle_kb_irq() {
    let mut port = KBD_PORT.write();
    let scancode: u8 = unsafe { port.read() };

    let event_type = if scancode & 0x80 == 0 {
        InputEventType::KeyPress(scancode)
    } else {
        InputEventType::KeyRelease(scancode & 0x7F)
    };

    let ev = InputEvent {
        timestamp: 0,
        event_type,
    };

    match event_type {
        InputEventType::KeyPress(scancode) => {
            let mut kb = Keyboard::new(layouts::Us104Key, ScancodeSet1);
            let event = kb.add_byte(scancode).unwrap().unwrap();
            let key = kb.process_keyevent(event);
            println!("{:?}", key.unwrap());
        }
        _ => ()
    }

    if let Some(dev) = &*KBD_DEVICE.read() {
        input_report_event(dev, ev);
    }
}

pub fn init_keyboard() {
    let dev = input_register_device("ps2_keyboard");
    *KBD_DEVICE.write() = Some(dev.clone());

    crate::println!("[kbd] PS/2 keyboard initialized on IRQ1");
}
