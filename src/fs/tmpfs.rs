use alloc::{boxed::Box, sync::Arc, vec::Vec};
use spin::RwLock;

use crate::fs::vfs::{
    Dentry, File, FileOperations, Inode, InodeOperations, SuperBlock, SuperBlockOperations,
};

pub fn find_dentry_by_inode(
    sb: Arc<RwLock<SuperBlock>>,
    target: Arc<RwLock<Inode>>,
) -> Option<Arc<RwLock<Dentry>>> {
    fn recurse(d: &Arc<RwLock<Dentry>>, target: &Arc<RwLock<Inode>>) -> Option<Arc<RwLock<Dentry>>> {
        let d_ref = d.read();

        if let Some(inode) = &d_ref.inode {
            if Arc::ptr_eq(inode, target) {
                return Some(d.clone());
            }
        }

        for child in &d_ref.children {
            if let Some(found) = recurse(child, target) {
                return Some(found);
            }
        }

        None
    }

    let root = sb.read().root.clone();
    recurse(&root, &target)
}

pub struct TmpfsSuperOps;
pub struct TmpfsInodeOps;
pub struct TmpfsFileOps;

impl SuperBlockOperations for TmpfsSuperOps {
    fn alloc_inode(&self, sb: Arc<RwLock<SuperBlock>>) -> Arc<RwLock<Inode>> {
        Arc::new(RwLock::new(Inode {
            ino: rand_ino(),
            size: 0,
            mode: 0o644,
            is_dir: false,
            super_block: sb.clone(),
            inode_ops: Some(Arc::new(TmpfsInodeOps)),
            file_ops: Some(Arc::new(TmpfsFileOps)),
            private_data: Some(Box::new(Vec::<u8>::new())),
        }))
    }

    fn destroy_inode(&self, _inode: Arc<RwLock<Inode>>) {}
}

impl InodeOperations for TmpfsInodeOps {
    fn lookup(&self, dir: Arc<RwLock<Inode>>, name: &str) -> Option<Arc<RwLock<Dentry>>> {
        let sb = dir.read().super_block.clone();
        let root = sb.read().root.clone();

        fn find_in(d: &Arc<RwLock<Dentry>>, name: &str) -> Option<Arc<RwLock<Dentry>>> {
            let dr = d.read();
            for child in &dr.children {
                let cr = child.read();
                if cr.name == name {
                    return Some(child.clone());
                }
                if cr.inode.as_ref().map(|i| i.read().is_dir).unwrap_or(false) {
                    if let Some(found) = find_in(child, name) {
                        return Some(found);
                    }
                }
            }
            None
        }

        find_in(&root, name)
    }

    fn create(&self, dir: Arc<RwLock<Inode>>, name: &str) -> Arc<RwLock<Dentry>> {
        let sb = dir.read().super_block.clone();
        let new_inode = sb.read().operations.as_ref().unwrap().alloc_inode(sb.clone());

        let new_dentry = Arc::new(RwLock::new(Dentry::new(name, Some(new_inode.clone()))));

        if let Some(parent_dentry) = find_dentry_by_inode(sb.clone(), dir.clone()) {
            parent_dentry.write().add_child(new_dentry.clone());
        } else {
            sb.read().root.write().add_child(new_dentry.clone());
        }

        new_dentry
    }
}
impl FileOperations for TmpfsFileOps {
    fn read(&self, file: &mut File, buf: &mut [u8]) -> usize {
        let inode_guard = file.inode.read();
        let data = inode_guard
            .private_data
            .as_ref()
            .unwrap()
            .downcast_ref::<Vec<u8>>()
            .unwrap();

        if file.position >= data.len() {
            return 0;
        }

        let end = core::cmp::min(file.position + buf.len(), data.len());
        let n = end - file.position;
        buf[..n].copy_from_slice(&data[file.position..end]);
        file.position += n;
        n
    }

    fn write(&self, file: &mut File, buf: &[u8]) -> usize {
        let mut inode_guard = file.inode.write();
        let data = inode_guard
            .private_data
            .as_mut()
            .unwrap()
            .downcast_mut::<Vec<u8>>()
            .unwrap();

        // если позиция не в конце — пишем в указанное место
        if file.position > data.len() {
            data.resize(file.position, 0);
        }

        if file.position == data.len() {
            data.extend_from_slice(buf);
        } else {
            let end = file.position + buf.len();
            if end > data.len() {
                data.resize(end, 0);
            }
            data[file.position..end].copy_from_slice(buf);
        }

        inode_guard.size = data.len();
        file.position += buf.len();
        buf.len()
    }

    fn open(&self, inode: Arc<RwLock<Inode>>) -> File {
        File {
            inode,
            position: 0,
            flags: 0,
            f_op: Some(Arc::new(TmpfsFileOps)),
        }
    }

    fn release(&self, _file: &File) {}
}

pub fn tmpfs_mount() -> Arc<RwLock<SuperBlock>> {
    let sb = Arc::new(RwLock::new(SuperBlock {
        root: Arc::new(RwLock::new(Dentry::new_root())),
        operations: Some(Arc::new(TmpfsSuperOps)),
        private_data: None,
    }));

    let root_inode = Arc::new(RwLock::new(Inode {
        ino: 0,
        size: 0,
        mode: 0o755,
        is_dir: true,
        super_block: sb.clone(),
        inode_ops: Some(Arc::new(TmpfsInodeOps)),
        file_ops: Some(Arc::new(TmpfsFileOps)),
        private_data: Some(Box::new(Vec::<u8>::new())),
    }));

    sb.write().root.write().inode = Some(root_inode);
    sb
}

/// Простейший генератор номеров inode
fn rand_ino() -> u64 {
    use core::sync::atomic::{AtomicU64, Ordering};
    static COUNTER: AtomicU64 = AtomicU64::new(1);
    COUNTER.fetch_add(1, Ordering::Relaxed)
}
