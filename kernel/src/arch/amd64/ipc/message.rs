use acpi::aml::object::ObjectType;

pub const MAX_CAPS_PER_MSG: usize = 4;

pub const MSG_DATA_WORDS: usize = 4;

#[derive(Clone, Copy, PartialEq, Eq)]
pub struct Rights(u64);

impl Rights {
    pub const NONE:  Rights = Rights(0);
    pub const READ:  Rights = Rights(1 << 0);
    pub const WRITE: Rights = Rights(1 << 1);
    pub const EXEC:  Rights = Rights(1 << 2);
    pub const GRANT: Rights = Rights(1 << 3); 
    pub const ALL:   Rights = Rights(0xF);

    pub fn contains(self, other: Rights) -> bool {
        (self.0 & other.0) == other.0
    }

    pub fn intersect(self, other: Rights) -> Rights {
        Rights(self.0 & other.0)
    }
}

pub const OBJ_TYPE_SHIFT: u64 = 56;
pub const OBJ_TYPE_TCB:      u64 = 1;
pub const OBJ_TYPE_ENDPOINT: u64 = 2;
pub const OBJ_TYPE_VSPACE:   u64 = 3;
pub const OBJ_TYPE_UNTYPED:  u64 = 4;
pub const OBJ_TYPE_CNODE:    u64 = 5;

#[derive(Clone, Copy, PartialEq, Eq, Hash)]
pub struct ObjectId(pub u64);

impl ObjectId {
    pub fn new(obj_type: u64, id: u64) -> Self {
        ObjectId((obj_type << OBJ_TYPE_SHIFT) | (id & 0x00FF_FFFF_FFFF_FFFF))
    }
    
    pub fn obj_type(&self) -> u64 {
        self.0 >> OBJ_TYPE_SHIFT
    }
    
    pub fn raw_id(&self) -> u64 {
        self.0 & 0x00FF_FFFF_FFFF_FFFF
    }
    
    pub fn is_tcb(&self) -> bool      { self.obj_type() == OBJ_TYPE_TCB }
    pub fn is_endpoint(&self) -> bool { self.obj_type() == OBJ_TYPE_ENDPOINT }
    pub fn is_vspace(&self) -> bool   { self.obj_type() == OBJ_TYPE_VSPACE }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(C)]
pub struct Capability {
    pub object: ObjectId,
    pub rights: Rights,
    pub depth:  u32,
    pub _pad:   u32,
}

impl Capability {
    pub const NULL: Capability = Capability {
        object: ObjectId(0),
        rights: Rights::NONE,
        depth:  0,
        _pad:   0,
    };

    pub fn new(object: ObjectId, rights: Rights) -> Self {
        Capability { object, rights, depth: 0, _pad: 0 }
    }

    pub fn is_null(&self) -> bool {
        self.object.0 == 0
    }

    pub fn derive(&self, requested: Rights) -> Option<Capability> {
        if !self.rights.contains(Rights::GRANT) {
            return None;
        }
        let child_rights = self.rights.intersect(requested);
        Some(Capability {
            object: self.object,
            rights: child_rights,
            depth:  self.depth + 1,
            _pad:   0,
        })
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
#[repr(transparent)]
pub struct MsgLabel(pub u64);

impl MsgLabel {
    pub const INVALID:    MsgLabel = MsgLabel(0);
    pub const REPLY_OK:   MsgLabel = MsgLabel(1);
    pub const REPLY_ERR:  MsgLabel = MsgLabel(2);
    pub const NOTIFY:     MsgLabel = MsgLabel(3);
    pub const CALL:       MsgLabel = MsgLabel(4);
    pub const USER_BASE:  MsgLabel = MsgLabel(0x1000);
}

#[derive(Clone, Copy)]
#[repr(C)]
pub struct FastMessage {
    pub label: MsgLabel,
    pub data:  [u64; MSG_DATA_WORDS],
    pub caps:  [Capability; MAX_CAPS_PER_MSG],
    pub n_caps: usize,
}

impl Default for FastMessage {
    fn default() -> Self {
        FastMessage {
            label: MsgLabel::INVALID,
            data:  [0u64; MSG_DATA_WORDS],
            caps:  [Capability::NULL; MAX_CAPS_PER_MSG],
            n_caps: 0,
        }
    }
}

impl FastMessage {
    pub const fn empty(label: MsgLabel) -> Self {
        FastMessage {
            label,
            data:   [0u64; MSG_DATA_WORDS],
            caps:   [Capability::NULL; MAX_CAPS_PER_MSG],
            n_caps: 0,
        }
    }

    pub fn with_data(label: MsgLabel, data: [u64; MSG_DATA_WORDS]) -> Self {
        FastMessage {
            label,
            data,
            caps:   [Capability::NULL; MAX_CAPS_PER_MSG],
            n_caps: 0,
        }
    }

    pub fn add_cap(&mut self, cap: Capability) -> bool {
        if self.n_caps >= MAX_CAPS_PER_MSG {
            return false;
        }
        self.caps[self.n_caps] = cap;
        self.n_caps += 1;
        true
    }

    pub fn caps(&self) -> &[Capability] {
        &self.caps[..self.n_caps]
    }
}