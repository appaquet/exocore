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

impl<FS: FileSystem + 'static> From<FS> for DynFileSystem {
    fn from(fs: FS) -> Self {
        DynFileSystem(Box::new(fs))
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
    IO(#[from] std::io::Error),

    #[error("File not found: {0}")]
    NotFound(PathBuf),

    #[error("path error: {0}")]
    Path(#[source] anyhow::Error),

    #[error("Not a OsFileSystem")]
    NotOsPath,
}

#[cfg(test)]
mod tests {
    use std::io::SeekFrom;

    use super::*;

    pub fn test_write_read_file(fs: impl Into<DynFileSystem>) {
        let fs = fs.into();
        {
            // cannot create empty path
            assert!(fs.open_write(Path::new("")).is_err());
            assert!(fs.open_read(Path::new("")).is_err());
            assert!(fs.stat(Path::new("")).is_err());

            // inexistent file
            assert!(fs.stat(Path::new("test")).is_err());
        }

        {
            // can create file
            assert!(!fs.exists(Path::new("file1")));

            let mut file = fs.open_write(Path::new("file1")).unwrap();
            file.write_all(b"Hello ").unwrap();
            file.write_all(b"world").unwrap();

            let stat = fs.stat(Path::new("file1")).unwrap();
            assert_eq!(stat.path(), Path::new("file1"));
            assert_eq!(stat.size(), 11);

            assert!(fs.exists(Path::new("file1")));
        }

        {
            // can read the file
            let mut file = fs.open_read(Path::new("file1")).unwrap();
            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("Hello world", buf);

            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("", buf);
        }

        {
            // can seek
            let mut file = fs.open_write(Path::new("file1")).unwrap();

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
            let fs = fs.clone();
            assert!(fs.exists(Path::new("file1")));
        }
    }

    pub fn test_list(fs: impl Into<DynFileSystem>) {
        let fs = fs.into();
        assert!(fs.list(None).unwrap().is_empty());
        assert!(fs.list(Some(Path::new(""))).unwrap().is_empty());

        {
            fs.open_write(Path::new("dir1/file1")).unwrap();
            fs.open_write(Path::new("dir1/file2")).unwrap();
            fs.open_write(Path::new("dir1/file3")).unwrap();
            fs.open_write(Path::new("dir2/file1")).unwrap();
            fs.open_write(Path::new("dir2/file2")).unwrap();
            fs.open_write(Path::new("file1")).unwrap();
        }

        assert_eq!(fs.list(Some(Path::new("dir1"))).unwrap().len(), 3);
        assert_eq!(fs.list(Some(Path::new("dir2"))).unwrap().len(), 2);
        assert_eq!(fs.list(Some(Path::new("file1"))).unwrap().len(), 1);
        assert_eq!(fs.list(Some(Path::new(""))).unwrap().len(), 6);
        assert_eq!(fs.list(None).unwrap().len(), 6);
        assert_eq!(fs.list(Some(Path::new("not/found"))).unwrap().len(), 0);

        // TODO: Validate path
    }

    pub fn test_delete(fs: impl Into<DynFileSystem>) {
        let fs = fs.into();
        {
            let mut file = fs.open_write(Path::new("test")).unwrap();
            file.write_all(b"Hello").unwrap();
        }

        assert!(fs.exists(Path::new("test")));

        fs.delete(Path::new("test")).unwrap();

        assert!(!fs.exists(Path::new("test")));
    }
}
