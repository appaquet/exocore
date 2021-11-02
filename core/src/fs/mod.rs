use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

pub mod os;
pub mod ram;
pub mod scoped;
#[cfg(target_arch = "wasm32")]
pub mod web;

pub trait FileSystem: Send + Sync {
    fn open_read(&self, path: &Path) -> Result<Box<dyn FileRead>, Error>;
    fn open_write(&self, path: &Path) -> Result<Box<dyn FileWrite>, Error>;
    fn list(&self, prefix: Option<&Path>) -> Result<Vec<Box<dyn FileStat>>, Error>;
    fn stat(&self, path: &Path) -> Result<Box<dyn FileStat>, Error>;
    fn exists(&self, path: &Path) -> bool;
    fn delete(&self, path: &Path) -> Result<(), Error>;
    fn clear(&self) -> Result<(), Error>;
    fn clone(&self) -> DynFileSystem;
    fn as_os_path(&self, path: &Path) -> Result<PathBuf, Error>;
}

pub struct DynFileSystem(pub Box<dyn FileSystem>);

impl Clone for DynFileSystem {
    fn clone(&self) -> Self {
        self.0.clone()
    }
}

impl Deref for DynFileSystem {
    type Target = dyn FileSystem;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

pub trait FileRead: std::io::Read + std::io::Seek {}

pub trait FileWrite: std::io::Write + std::io::Read + std::io::Seek {}

pub trait FileStat {
    fn path(&self) -> &Path;
    fn size(&self) -> u64;
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[source] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(PathBuf),

    #[error("Cannot create file at root")]
    InvalidFilePathRoot,

    #[error("Not a OsFileSystem")]
    NotOsPath,
}
