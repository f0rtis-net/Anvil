#[derive(Clone, Copy, Debug)]
pub enum InputEventType {
    KeyPress(u8),   
    KeyRelease(u8),
}

#[derive(Clone, Copy, Debug)]
pub struct InputEvent {
    pub timestamp: u64,
    pub event_type: InputEventType,
}