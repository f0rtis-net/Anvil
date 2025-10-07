use alloc::{boxed::Box, string::String, sync::Arc, vec::Vec};
use spin::RwLock;

use crate::println;

pub trait SuperBlockOperations: Send + Sync {
    fn alloc_inode(&self, sb: Arc<RwLock<SuperBlock>>) -> Arc<RwLock<Inode>>;
    fn destroy_inode(&self, inode: Arc<RwLock<Inode>>);
}

pub trait InodeOperations: Send + Sync {
    fn lookup(&self, dir: Arc<RwLock<Inode>>, name: &str) -> Option<Arc<RwLock<Dentry>>>;
    fn create(&self, dir: Arc<RwLock<Inode>>, name: &str) -> Arc<RwLock<Dentry>>;
}

pub trait FileOperations: Send + Sync {
    fn read(&self, file: &mut File, buf: &mut [u8]) -> usize;
    fn write(&self, file: &mut File, buf: &[u8]) -> usize;
    fn open(&self, inode: Arc<RwLock<Inode>>) -> File;
    fn release(&self, file: &File);
}

pub struct SuperBlock {
    pub root: Arc<RwLock<Dentry>>,
    pub operations: Option<Arc<dyn SuperBlockOperations>>,
    pub private_data: Option<Box<dyn core::any::Any + Send + Sync>>,
}

pub struct Inode {
    pub ino: u64,
    pub size: usize,
    pub mode: u32,
    pub super_block: Arc<RwLock<SuperBlock>>,
    pub is_dir: bool,
    pub inode_ops: Option<Arc<dyn InodeOperations>>,
    pub file_ops: Option<Arc<dyn FileOperations>>,

    pub private_data: Option<Box<dyn core::any::Any + Send + Sync>>,
}

pub struct Dentry {
    pub name: String,
    pub inode: Option<Arc<RwLock<Inode>>>,
    pub parent: Option<Arc<RwLock<Dentry>>>,
    pub children: Vec<Arc<RwLock<Dentry>>>,
}

pub struct File {
    pub inode: Arc<RwLock<Inode>>,
    pub position: usize,
    pub flags: u32,
    pub f_op: Option<Arc<dyn FileOperations>>,
}

impl SuperBlock {
    pub fn new() -> Arc<RwLock<Self>> {
        Arc::new(RwLock::new(Self {
            root: Arc::new(RwLock::new(Dentry::new_root())),
            operations: None,
            private_data: None,
        }))
    }
}

impl Dentry {
    pub fn new(name: &str, inode: Option<Arc<RwLock<Inode>>>) -> Self {
        Dentry {
            name: name.into(),
            inode,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn new_root() -> Self {
        Dentry {
            name: "/".into(),
            inode: None,
            parent: None,
            children: Vec::new(),
        }
    }

    pub fn add_child(&mut self, child: Arc<RwLock<Dentry>>) {
        self.children.push(child);
    }
}

pub fn vfs_create(parent: Arc<RwLock<Inode>>, name: &str) -> Arc<RwLock<Dentry>> {
    let sb = parent.read().super_block.clone();
    let inode_ops = parent.read().inode_ops.clone();

    if let Some(ops) = inode_ops {
        ops.create(parent.clone(), name)
    } else {
        let sb_guard = sb.read();
        let new_inode = if let Some(ref ops) = sb_guard.operations {
            ops.alloc_inode(sb.clone())
        } else {
            panic!("Superblock has no alloc_inode operation");
        };

        let dentry = Arc::new(RwLock::new(Dentry::new(name, Some(new_inode))));
        sb_guard.root.write().add_child(dentry.clone());
        dentry
    }
}

pub fn vfs_open(dentry: Arc<RwLock<Dentry>>) -> File {
    let inode = dentry.read().inode.clone().expect("No inode for dentry");
    let inode_ref = inode.read();
    if let Some(ref ops) = inode_ref.file_ops {
        ops.open(inode.clone())
    } else {
        panic!("Inode has no file_ops::open");
    }
}

pub fn vfs_write(file: &mut File, data: &[u8]) -> usize {
    let ops = file.f_op.clone();
    if let Some(ref ops) = ops {
        ops.write(file, data)
    } else {
        0
    }
}

pub fn vfs_read(file: &mut File, buf: &mut [u8]) -> usize {
    let ops = file.f_op.clone();
    if let Some(ref ops) = ops {
        ops.read(file, buf)
    } else {
        0
    }
}

