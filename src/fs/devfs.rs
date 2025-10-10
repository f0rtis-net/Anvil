use alloc::string::ToString;
use alloc::{collections::btree_map::BTreeMap, string::String, sync::Arc, vec::Vec};
use alloc::vec;
use spin::Mutex;

use crate::fs::vfs::{FileDescriptor, FileFlags, FileType, Vfs, VfsError};

pub struct Inode {
    pub id: usize,
    pub file_type: FileType,
    pub size: usize,
    pub data: Option<Vec<u8>>, 
    pub children: Option<BTreeMap<String, Arc<Mutex<Inode>>>>, 
}

pub struct Devfs {
    inode_counter: usize,
    inodes: Arc<Mutex<BTreeMap<String, Arc<Mutex<Inode>>>>>,
}

#[derive(Debug)]
pub enum DeviceType {
    Null,
    Zero,
}

impl Devfs {
    pub fn new() -> Self {
        let mut devfs = Devfs {
            inode_counter: 1,
            inodes: Arc::new(Mutex::new(BTreeMap::new())),
        };

        let inode = devfs.create_device_inode(DeviceType::Null);
        devfs.inodes.lock().insert("/dev/zero".to_string(), inode);

        devfs
    }
    

    fn create_device_inode(&mut self, device_type: DeviceType) -> Arc<Mutex<Inode>> {
        let inode = Arc::new(Mutex::new(Inode {
            id: self.inode_counter,
            file_type: FileType::Device,
            size: 0,
            data: Some(match device_type {
                DeviceType::Null => vec![0],
                DeviceType::Zero => vec![0],
            }),
            children: None,
        }));
        self.inode_counter += 1;
        inode
    }
}

impl Vfs for Devfs {
    fn open(&self, path: &str, flags: FileFlags) -> Result<FileDescriptor, VfsError> {
        let inodes = self.inodes.lock();
        if let Some(inode) = inodes.get(path) {
            return Ok(inode.lock().id);
        }
        Err(VfsError::FileNotFound)
    }

    fn close(&self, fd: FileDescriptor) -> Result<(), VfsError> {
        Ok(())
    }

    fn read(&self, fd: FileDescriptor, buffer: &mut [u8]) -> Result<usize, VfsError> {
        let inodes = self.inodes.lock();
        for inode in inodes.values() {
            let inode = inode.lock();
            if inode.id == fd {
                if let Some(ref data) = inode.data {
                    let len = data.len().min(buffer.len());
                    buffer[..len].copy_from_slice(&data[..len]);
                    return Ok(len);
                }
            }
        }
        Err(VfsError::InvalidFileDescriptor)
    }

    fn write(&self, fd: FileDescriptor, buffer: &[u8]) -> Result<usize, VfsError> {
        Ok(0) 
    }

    fn mkdir(&self, path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotImplemented)
    }

    fn rmdir(&self, path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotImplemented)
    }

    fn remove(&self, path: &str) -> Result<(), VfsError> {
        Err(VfsError::NotImplemented)
    }
}
