use alloc::string::ToString;
use lazy_static::lazy_static;

use alloc::{string::String, sync::Arc, vec::Vec};
use alloc::vec;
use spin::{Mutex, RwLock}; 

use crate::{fs::{
    tmpfs::tmpfs_mount,
    vfs::{vfs_create, vfs_open, vfs_read, vfs_write, Dentry, File, SuperBlock},
}, println};

pub mod tmpfs;
pub mod devfs;
pub mod vfs;

fn normalize_path(path: &str) -> String {
    let mut cleaned = String::new();
    let mut last_was_slash = false;

    for c in path.chars() {
        if c == '/' {
            if !last_was_slash {
                cleaned.push('/');
                last_was_slash = true;
            }
        } else {
            cleaned.push(c);
            last_was_slash = false;
        }
    }

    if cleaned.len() > 1 && cleaned.ends_with('/') {
        cleaned.pop();
    }

    if cleaned.is_empty() {
        "/".to_string()
    } else {
        cleaned
    }
}

pub struct Mount {
    pub path: String,
    pub super_block: Arc<RwLock<SuperBlock>>,
}

pub struct GeneralVfsApi {
    mounts: Mutex<Vec<Mount>>,
}

impl GeneralVfsApi {
    pub fn init_vfs() -> GeneralVfsApi {
        GeneralVfsApi {
            mounts: Mutex::new(Vec::new()),
        }
    }

    pub fn mount_fs(&self, mount_point: Mount) {
        let mut guard = self.mounts.lock();
        guard.push(mount_point);
    }

    fn find_mount_for_path(&self, path: &str) -> Option<(Arc<RwLock<SuperBlock>>, String)> {
        let guard = self.mounts.lock();
        if guard.is_empty() {
            return None;
        }
        let normalized = normalize_path(path);

        let mut best: Option<&Mount> = None;

        for mount in guard.iter() {
            let mnt_path = if mount.path == "/" {
                "/"
            } else {
                mount.path.trim_end_matches('/')
            };

            if normalized == mnt_path || normalized.starts_with(&(mnt_path.to_string() + "/")) {
                if best.map_or(true, |b| mnt_path.len() > b.path.len()) {
                    best = Some(mount);
                }
            }
        }

        best.map(|m| {
            let rel_path = normalized
                .strip_prefix(&m.path)
                .unwrap_or(&normalized)
                .trim_start_matches('/')
                .to_string();
            (m.super_block.clone(), rel_path)
        })
    }

    pub fn create(&self, path: &str) -> Option<Arc<RwLock<Dentry>>> {
        let (sb, rel_path) = self.find_mount_for_path(path)?;
        let root_inode = sb.read().root.read().inode.clone().unwrap();
        Some(vfs_create(root_inode, &rel_path))
    }

    pub fn open(&self, path: &str) -> Option<File> {
        let (sb, rel_path) = self.find_mount_for_path(path)?;
        let binding = sb.read();
        let root = binding.root.read();

        for child in &root.children {
            println!("{:?}", child.read().name);
            if child.read().name == rel_path {
                return Some(vfs_open(child.clone()));
            }
        }
        None
    }

    pub fn write(&self, path: &str, data: &[u8]) -> usize {
        if let Some(mut file) = self.open(path) {
            vfs_write(&mut file, data)
        } else if let Some(dentry) = self.create(path) {
            let mut file = vfs_open(dentry);
            vfs_write(&mut file, data)
        } else {
            0
        }
    }

    pub fn read(&self, path: &str) -> Option<Vec<u8>> {
        let mut buf = vec![0u8; 4096];
        if let Some(mut file) = self.open(path) {
            let n = vfs_read(&mut file, &mut buf);
            buf.truncate(n);
            Some(buf)
        } else {
            None
        }
    }

    pub fn list_dir(&self, path: &str) {
        if let Some((sb, rel_path)) = self.find_mount_for_path(path) {
            let root = sb.read().root.clone();
            let dir = if rel_path.is_empty() {
                root
            } else {
                let parts: Vec<&str> = rel_path.split('/')
                    .filter(|s| !s.is_empty())
                    .collect();
                let mut current = root.clone();
                for p in parts {
                    let mut found = None;
                    {
                        let r = current.read();
                        for c in &r.children {
                            if c.read().name == p {
                                found = Some(c.clone());
                                break;
                            }
                        }
                    }
                    if let Some(next) = found {
                        current = next;
                    } else {
                        println!("Directory not found: {}", p);
                        return;
                    }
                }
                current
            };

            let d = dir.read();
            println!("Contents of {}:", path);
            for c in &d.children {
                let inode = c.read().inode.clone().unwrap();
                if inode.read().is_dir {
                    println!("[DIR]  {}", c.read().name);
                } else {
                    println!("[FILE] {}", c.read().name);
                }
            }
        } else {
            println!("Directory not found: {}", path);
        }
    }
}

lazy_static! {
    static ref VFS_API: GeneralVfsApi = GeneralVfsApi::init_vfs();
}

pub fn mount_and_test() {
    let sb = tmpfs_mount(); 
    let mount = Mount { path: "/".into(), super_block: sb.clone() };

    VFS_API.mount_fs(mount);

    VFS_API.write("/hello.txt", b"Hello from tmpfs!");

    println!("{:?}", VFS_API.list_dir("/"));
}
