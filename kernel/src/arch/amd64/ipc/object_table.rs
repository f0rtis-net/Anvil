use core::sync::atomic::{AtomicU32, Ordering};
use spin::Mutex;

use crate::arch::amd64::scheduler::task::TaskIdIndex;


#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u16)]
pub enum KernelObjType {
    VSpace   = 0,
    Endpoint = 1,
    Frame    = 2,
    Thread   = 3,
    Irq      = 4,
    CNode    = 5,
}

pub enum ObjData {
    VSpace(TaskIdIndex),
    Endpoint(TaskIdIndex),
    CNode(TaskIdIndex),
    Thread(TaskIdIndex)
}

pub struct KernelObject {
    pub obj_type: KernelObjType,
    pub refcount: AtomicU32,
    pub data: ObjData,
}

impl KernelObject {
    pub fn new(obj_type: KernelObjType, data: ObjData) -> Self {
        Self {
            obj_type,
            refcount: AtomicU32::new(1),
            data,
        }
    }

    pub fn inc_ref(&self) -> u32 {
        self.refcount.fetch_add(1, Ordering::Relaxed)
    }

    pub fn dec_ref(&self) -> bool {
        self.refcount.fetch_sub(1, Ordering::Release) == 1
    }
}

const MAX_OBJECTS: usize = 4096;

pub type ObjHandle = u32;

struct Slot {
    generation: u32, 
    obj: Option<KernelObject>,
}

pub struct ObjectTable {
    slots: [Slot; MAX_OBJECTS],
    free_head: usize,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HandleRef {
    pub index: u16,
    pub generation: u32,
}

impl ObjectTable {
    pub const fn new() -> Self {
        const EMPTY_SLOT: Slot = Slot { generation: 0, obj: None };
        Self {
            slots: [EMPTY_SLOT; MAX_OBJECTS],
            free_head: 0,
        }
    }

    pub fn insert(&mut self, obj: KernelObject) -> Result<HandleRef, ()> {
        for i in self.free_head..MAX_OBJECTS {
            if self.slots[i].obj.is_none() {
                let generation = self.slots[i].generation;
                self.slots[i].obj = Some(obj);
                self.free_head = i + 1;
                return Ok(HandleRef { index: i as u16, generation });
            }
        }
        Err(())
    }

    pub fn get(&self, handle: HandleRef) -> Option<&KernelObject> {
        let slot = &self.slots[handle.index as usize];
        if slot.generation == handle.generation {
            slot.obj.as_ref()
        } else {
            None
        }
    }

    pub fn get_mut(&mut self, handle: HandleRef) -> Option<&mut KernelObject> {
        let slot = &mut self.slots[handle.index as usize];
        if slot.generation == handle.generation {
            slot.obj.as_mut()
        } else {
            None
        }
    }

    pub fn remove(&mut self, handle: HandleRef) -> Option<KernelObject> {
        let slot = &mut self.slots[handle.index as usize];
        if slot.generation == handle.generation && slot.obj.is_some() {
            slot.generation = slot.generation.wrapping_add(1); 
            if handle.index as usize <= self.free_head {
                self.free_head = handle.index as usize;
            }
            slot.obj.take()
        } else {
            None
        }
    }
}

static OBJECT_TABLE: Mutex<ObjectTable> = Mutex::new(ObjectTable::new());

pub fn obj_insert(obj: KernelObject) -> Result<HandleRef, ()> {
    OBJECT_TABLE.lock().insert(obj)
}

pub fn with_object<F, R>(h: HandleRef, f: F) -> Option<R>
where
    F: FnOnce(&KernelObject) -> R,
{
    let table = OBJECT_TABLE.lock();
    table.get(h).map(f)
}

pub fn with_object_mut<F, R>(h: HandleRef, f: F) -> Option<R>
where
    F: FnOnce(&mut KernelObject) -> R,
{
    let mut table = OBJECT_TABLE.lock();
    table.get_mut(h).map(f)
}