use crate::arch::amd64::{ipc::message::{Capability, Rights}, scheduler::task::Task};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapError {
    InvalidIdx,
    WrongType,
    WrongOwner,
    InsufficientRights,
}

impl CapError {
    pub fn as_syscall_err(self) -> u64 {
        match self {
            CapError::InvalidIdx         => u64::MAX,
            CapError::WrongType          => u64::MAX - 1,
            CapError::WrongOwner         => u64::MAX - 2,
            CapError::InsufficientRights => u64::MAX - 3,
        }
    }
}

pub enum ExpectedOwner {
    CurrentTask,
    Specific(u64),
    Any,
}

pub fn resolve_cap(
    task: &Task,
    cap_idx: u64,
    expected_type: u64,
    expected_owner: ExpectedOwner,
    required_rights: Rights,
) -> Result<Capability, CapError> {
    let cnode = task.tcb.cnode.lock();

    let cap = cnode.get(cap_idx as u32)
        .copied()
        .ok_or(CapError::InvalidIdx)?;

    if cap.object.obj_type() != expected_type {
        return Err(CapError::WrongType);
    }

    match expected_owner {
        ExpectedOwner::CurrentTask => {
            if cap.object.raw_id() != task.id.id() as u64 {
                return Err(CapError::WrongOwner);
            }
        }
        ExpectedOwner::Specific(id) => {
            if cap.object.raw_id() != id {
                return Err(CapError::WrongOwner);
            }
        }
        ExpectedOwner::Any => {}
    }

    if !cap.rights.contains(required_rights) {
        return Err(CapError::InsufficientRights);
    }

    Ok(cap)
}