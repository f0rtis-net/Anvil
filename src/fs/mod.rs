use alloc::{collections::btree_map::BTreeMap, string::{String, ToString}, sync::Arc};
use spin::Mutex;
use lazy_static::lazy_static;
use crate::{fs::{devfs::Devfs, vfs::{FileFlags, Vfs, VfsError}}, println};

pub mod devfs;
pub mod vfs;

pub struct MountPoint {
    pub mount_path: String,
    pub fs: Arc<dyn Vfs>,  
}

pub struct VirtualFileSystem {
    mount_points: Arc<Mutex<BTreeMap<String, MountPoint>>>,
}

impl VirtualFileSystem {
    pub fn new() -> Self {
        VirtualFileSystem {
            mount_points: Arc::new(Mutex::new(BTreeMap::new())),
        }
    }

    fn mount(&mut self, path: &str, fs: Arc<dyn Vfs>) -> Result<(), VfsError> {
        let mut mount_points = self.mount_points.lock();
        if mount_points.contains_key(path) {
            return Err(VfsError::FileAlreadyExists); 
        }
        mount_points.insert(
            path.to_string(),
            MountPoint {
                mount_path: path.to_string(),
                fs,
            },
        );
        Ok(())
    }

    fn get_fs_at(&self, path: &str) -> Option<Arc<dyn Vfs>> {
        let mount_points = self.mount_points.lock();
        mount_points.get(path).map(|mount_point| mount_point.fs.clone())
    }
}


pub fn test() {
    println!("MOUNTING DEV FS");
    let devfs = Arc::new(Devfs::new());
    let mut fs = VirtualFileSystem::new();

    fs.mount("/dev", devfs).unwrap();

    if let Some(devfs1) = fs.get_fs_at("/dev") {
        println!("Dev fs was mounted");

        let file = devfs1.open("/dev/zero", vfs::FileFlags::Read).unwrap();
    }
}