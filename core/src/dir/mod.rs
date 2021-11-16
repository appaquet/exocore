use std::{
    ops::Deref,
    path::{Path, PathBuf},
};

use self::scoped::ScopedDirectory;

pub mod os;
pub mod ram;
pub mod scoped;
#[cfg(feature = "web")]
pub mod web;

pub trait Directory: Send + Sync {
    fn open_read(&self, path: &Path) -> Result<Box<dyn FileRead>, Error>;
    fn open_write(&self, path: &Path) -> Result<Box<dyn FileWrite>, Error>;
    fn list(&self, prefix: Option<&Path>) -> Result<Vec<Box<dyn FileStat>>, Error>;
    fn stat(&self, path: &Path) -> Result<Box<dyn FileStat>, Error>;
    fn exists(&self, path: &Path) -> bool;
    fn delete(&self, path: &Path) -> Result<(), Error>;
    fn clone(&self) -> DynDirectory;
    fn as_os_path(&self) -> Result<PathBuf, Error>;

    fn scope(&self, path: PathBuf) -> Result<DynDirectory, Error> {
        let dir = self.clone();
        Ok(ScopedDirectory::new(dir, path).into())
    }
}

pub struct DynDirectory(pub Box<dyn Directory>);

impl Clone for DynDirectory {
    fn clone(&self) -> Self {
        self.0.clone()
    }
}

impl Deref for DynDirectory {
    type Target = dyn Directory;

    fn deref(&self) -> &Self::Target {
        self.0.as_ref()
    }
}

impl<D: Directory + 'static> From<D> for DynDirectory {
    fn from(dir: D) -> Self {
        DynDirectory(Box::new(dir))
    }
}

pub trait FileRead: std::io::Read + std::io::Seek + Send {}

pub trait FileWrite: std::io::Write + std::io::Read + std::io::Seek + Send {}

pub trait FileStat {
    fn path(&self) -> &Path;
    fn size(&self) -> u64;
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("IO error: {0}")]
    IO(#[from] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(PathBuf),

    #[error("path error: {0}")]
    Path(#[source] anyhow::Error),

    #[error("Not a OsDirectory")]
    NotOsDirectory,

    #[error("Other: {0}")]
    Other(#[from] anyhow::Error),
}

#[cfg(test)]
mod tests {
    use std::io::SeekFrom;

    use super::*;

    pub fn test_write_read_file(dir: impl Into<DynDirectory>) {
        let dir = dir.into();
        {
            // cannot create empty path
            assert!(dir.open_write(Path::new("")).is_err());
            assert!(dir.open_read(Path::new("")).is_err());
            assert!(dir.stat(Path::new("")).is_err());

            // inexistent file
            assert!(dir.stat(Path::new("test")).is_err());
        }

        {
            // can create file
            assert!(!dir.exists(Path::new("file1")));

            let mut file = dir.open_write(Path::new("file1")).unwrap();
            file.write_all(b"Hello ").unwrap();
            file.write_all(b"world").unwrap();

            let stat = dir.stat(Path::new("file1")).unwrap();
            assert_eq!(stat.path(), Path::new("file1"));
            assert_eq!(stat.size(), 11);

            assert!(dir.exists(Path::new("file1")));
        }

        {
            // can read the file
            let mut file = dir.open_read(Path::new("file1")).unwrap();
            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("Hello world", buf);

            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("", buf);
        }

        {
            // can seek
            let mut file = dir.open_write(Path::new("file1")).unwrap();

            file.seek(SeekFrom::Start(6)).unwrap();
            file.write_all(b"monde").unwrap();

            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("", buf);

            file.seek(SeekFrom::Start(0)).unwrap();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("Hello monde", buf);

            file.seek(SeekFrom::End(-5)).unwrap();
            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("monde", buf);

            file.seek(SeekFrom::Current(-5)).unwrap();
            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("monde", buf);
        }

        {
            // can clone
            #[allow(clippy::redundant_clone)]
            let dir = dir.clone();
            assert!(dir.exists(Path::new("file1")));
        }
    }

    pub fn test_list(dir: impl Into<DynDirectory>) {
        let dir = dir.into();
        assert!(dir.list(None).unwrap().is_empty());
        assert!(dir.list(Some(Path::new(""))).unwrap().is_empty());

        {
            dir.open_write(Path::new("dir1/file1")).unwrap();
            dir.open_write(Path::new("dir1/file2")).unwrap();
            dir.open_write(Path::new("dir1/file3")).unwrap();
            dir.open_write(Path::new("dir2/file1")).unwrap();
            dir.open_write(Path::new("dir2/file2")).unwrap();
            dir.open_write(Path::new("file1")).unwrap();
        }

        assert_eq!(dir.list(Some(Path::new("dir1"))).unwrap().len(), 3);
        assert_eq!(dir.list(Some(Path::new("dir2"))).unwrap().len(), 2);
        assert_eq!(dir.list(Some(Path::new("file1"))).unwrap().len(), 1);
        assert_eq!(dir.list(Some(Path::new(""))).unwrap().len(), 6);
        assert_eq!(dir.list(None).unwrap().len(), 6);
        assert_eq!(dir.list(Some(Path::new("not/found"))).unwrap().len(), 0);

        // TODO: Validate path
    }

    pub fn test_delete(dir: impl Into<DynDirectory>) {
        let dir = dir.into();
        {
            let mut file = dir.open_write(Path::new("test")).unwrap();
            file.write_all(b"Hello").unwrap();
        }

        assert!(dir.exists(Path::new("test")));

        dir.delete(Path::new("test")).unwrap();

        assert!(!dir.exists(Path::new("test")));
    }
}
