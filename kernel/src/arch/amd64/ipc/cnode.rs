use crate::arch::amd64::ipc::message::Capability;

//TODO, BUG: when set to 256 - kernel crash :(
pub const CAPABILITY_MAX: usize = 50;
pub type CapIdx = u32;

pub struct CNode {
    slots: [Capability; CAPABILITY_MAX]
}

impl CNode {
    pub fn new() -> Self {
        Self {
            slots: [Capability::NULL; CAPABILITY_MAX]
        }
    }

    pub fn insert_at(&mut self, idx: CapIdx, cap: Capability) {
        self.slots[idx as usize] = cap;
    }

    pub fn get(&self, idx: CapIdx) -> Option<&Capability> {
        if idx >= CAPABILITY_MAX as u32 {
            return None;
        }

        let cap = &self.slots[idx as usize];
        if cap.is_null() { None } else { Some(cap) }
    }
    
    pub fn find_free(&self) -> Option<CapIdx> {
        self.slots.iter()
            .enumerate()
            .find(|(_, c)| c.is_null())
            .map(|(i, _)| i as CapIdx)
    }
    
    pub fn alloc(&mut self, cap: Capability) -> Option<CapIdx> {
        let idx = self.find_free()?;
        self.slots[idx as usize] = cap;
        Some(idx)
    }
    
    pub fn delete(&mut self, idx: CapIdx) {
        self.slots[idx as usize] = Capability::NULL;
    }
}