use std::{fmt::Write, ops::Deref, path::PathBuf};

pub mod os;
pub mod ram;

#[cfg(target_arch = "wasm32")]
pub mod web;

pub trait FileSystem: Send + Sync {
    fn open_read(&self, path: &Path) -> Result<Box<dyn FileRead>, Error>;
    fn open_write(&self, path: &Path) -> Result<Box<dyn FileWrite>, Error>;

    // TOOD: List prefix
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

pub struct ScopedFilesystem {
    root: DynFileSystem,
    prefix: Path,
}

impl ScopedFilesystem {
    // TODO: should allow resolving to os path
}

#[derive(Debug, PartialEq, Eq, PartialOrd, Ord, Hash, Clone)]
pub struct Path {
    components: Vec<String>, // TODO: Could be a smallvec
}

impl Path {
    pub fn new() -> Path {
        Path {
            components: Vec::new(),
        }
    }

    pub fn is_root(&self) -> bool {
        self.components.is_empty()
    }
}

impl Default for Path {
    fn default() -> Self {
        Self::new()
    }
}

impl From<&str> for Path {
    fn from(s: &str) -> Self {
        Path {
            components: s
                .split('/')
                .filter(|c| !c.is_empty())
                .map(|c| c.to_string())
                .collect(),
        }
    }
}

impl From<String> for Path {
    fn from(s: String) -> Self {
        s.as_str().into()
    }
}

impl std::fmt::Display for Path {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> Result<(), std::fmt::Error> {
        if self.components.is_empty() {
            f.write_char('/')?;
        }

        for component in &self.components {
            f.write_char('/')?;
            f.write_str(component)?;
        }

        Ok(())
    }
}

pub struct FileStat {
    path: Path,
    // TODO: Size
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[source] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(Path),

    #[error("Cannot create file at root")]
    InvalidFilePathRoot,

    #[error("Not a OsFileSystem")]
    NotOsPath,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_path() {
        let p = Path::from("/c1/c1");
        assert_eq!(p.to_string(), "/c1/c1");

        let p = Path::new();
        assert_eq!(p.to_string(), "/");

    }
}
