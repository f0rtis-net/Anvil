use alloc::{sync::Arc, vec::Vec};
use spin::RwLock;

use crate::bdrivers::input::events::InputEvent;

mod events;
pub mod ps2_kb;

pub trait InputDriver: Send + Sync {
    fn name(&self) -> &'static str;
    fn read_event(&self) -> Option<InputEvent>;
}

pub struct InputDevice {
    pub name: &'static str,
    pub id: usize,
    pub events: Arc<RwLock<Vec<InputEvent>>>,
}

impl InputDevice {
    pub fn new(name: &'static str, id: usize) -> Self {
        Self {
            name,
            id,
            events: Arc::new(RwLock::new(Vec::new())),
        }
    }

    pub fn push_event(&self, ev: InputEvent) {
        let mut queue = self.events.write();
        queue.push(ev);
        if queue.len() > 256 {
            queue.remove(0);
        }
    }

    pub fn pop_event(&self) -> Option<InputEvent> {
        let mut queue = self.events.write();
        if queue.is_empty() {
            None
        } else {
            Some(queue.remove(0))
        }
    }
}

lazy_static::lazy_static! {
    static ref INPUT_DEVICES: RwLock<Vec<Arc<InputDevice>>> = RwLock::new(Vec::new());
}

pub fn input_register_device(name: &'static str) -> Arc<InputDevice> {
    let mut list = INPUT_DEVICES.write();
    let id = list.len();
    let dev = Arc::new(InputDevice::new(name, id));
    list.push(dev.clone());
    dev
}

pub fn input_report_event(dev: &Arc<InputDevice>, event: InputEvent) {
    dev.push_event(event);
}

pub fn input_read_all() -> Vec<InputEvent> {
    let list = INPUT_DEVICES.read();
    let mut out = Vec::new();
    for d in list.iter() {
        let mut q = d.events.write();
        out.extend(q.drain(..));
    }
    out
}
