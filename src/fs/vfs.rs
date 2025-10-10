use alloc::{string::String, vec::Vec};

#[derive(Debug)]
pub enum VfsError {
    FileNotFound,
    PermissionDenied,
    InvalidFileDescriptor,
    NotImplemented,
    FileAlreadyExists,
    IOError,
}

#[derive(Debug)]
pub enum FileFlags {
    Read,
    Write,
    ReadWrite,
    Append,
}

#[derive(Debug)]
pub enum FileType {
    RegularFile,
    Directory,
    Device,
}


pub type FileDescriptor = usize;

pub trait FileOps {
    fn read(&self, fd: FileDescriptor, buffer: &mut [u8]) -> Result<usize, VfsError>;
    fn write(&self, fd: FileDescriptor, buffer: &[u8]) -> Result<usize, VfsError>;
    fn seek(&self, fd: FileDescriptor, offset: isize) -> Result<usize, VfsError>;
    fn close(&self, fd: FileDescriptor) -> Result<(), VfsError>;
}

pub trait DirectoryOps {
    fn create(&self, path: &str) -> Result<(), VfsError>;
    fn remove(&self, path: &str) -> Result<(), VfsError>;
    fn list(&self, path: &str) -> Result<Vec<String>, VfsError>;
}

pub trait Vfs {
    fn open(&self, path: &str, flags: FileFlags) -> Result<FileDescriptor, VfsError>;
    fn close(&self, fd: FileDescriptor) -> Result<(), VfsError>;
    fn read(&self, fd: FileDescriptor, buffer: &mut [u8]) -> Result<usize, VfsError>;
    fn write(&self, fd: FileDescriptor, buffer: &[u8]) -> Result<usize, VfsError>;
    fn mkdir(&self, path: &str) -> Result<(), VfsError>;
    fn rmdir(&self, path: &str) -> Result<(), VfsError>;
    fn remove(&self, path: &str) -> Result<(), VfsError>;
}

