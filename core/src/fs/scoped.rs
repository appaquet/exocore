use std::path::{Path, PathBuf};

use super::{DynFileSystem, Error, FileStat, FileSystem};

pub struct ScopedFileSystem {
    inner: DynFileSystem,
    base_path: PathBuf,
}

impl ScopedFileSystem {
    pub fn new(fs: impl Into<DynFileSystem>, base_path: PathBuf) -> Self {
        ScopedFileSystem {
            inner: fs.into(),
            base_path,
        }
    }

    fn join_path(&self, path: &Path, expect_file: bool) -> Result<PathBuf, Error> {
        let joined = self.base_path.join(path);
        if expect_file && path.parent().is_none() {
            return Err(Error::Path(anyhow!("expected a non-root path to a file")));
        }

        Ok(joined)
    }
}

impl FileSystem for ScopedFileSystem {
    fn open_read(&self, path: &std::path::Path) -> Result<Box<dyn super::FileRead>, super::Error> {
        let path = self.join_path(path, true)?;
        self.inner.open_read(&path)
    }

    fn open_write(
        &self,
        path: &std::path::Path,
    ) -> Result<Box<dyn super::FileWrite>, super::Error> {
        let path = self.join_path(path, true)?;
        self.inner.open_write(&path)
    }

    fn list(
        &self,
        prefix: Option<&std::path::Path>,
    ) -> Result<Vec<Box<dyn super::FileStat>>, super::Error> {
        let path = prefix.map(|p| self.join_path(p, false)).transpose()?;
        self.inner.list(path.as_deref())
    }

    fn stat(&self, path: &std::path::Path) -> Result<Box<dyn super::FileStat>, super::Error> {
        let resolved_path = self.join_path(path, true)?;
        let stat = self.inner.stat(&resolved_path)?;

        Ok(Box::new(ScopedFileStat {
            path: path.to_path_buf(),
            inner: stat,
        }))
    }

    fn exists(&self, path: &std::path::Path) -> bool {
        let path = if let Ok(path) = self.join_path(path, true) {
            path
        } else {
            return false;
        };

        self.inner.exists(&path)
    }

    fn delete(&self, path: &std::path::Path) -> Result<(), super::Error> {
        let path = self.join_path(path, true)?;
        self.inner.delete(&path)
    }

    fn clone(&self) -> DynFileSystem {
        ScopedFileSystem {
            inner: self.inner.clone(),
            base_path: self.base_path.clone(),
        }
        .into()
    }

    fn as_os_path(&self, path: &std::path::Path) -> Result<std::path::PathBuf, super::Error> {
        let path = self.join_path(path, false)?;
        self.inner.as_os_path(&path)
    }
}

pub struct ScopedFileStat {
    path: PathBuf,
    inner: Box<dyn super::FileStat>,
}

impl FileStat for ScopedFileStat {
    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn size(&self) -> u64 {
        self.inner.size()
    }
}

#[cfg(test)]
mod tests {
    use crate::fs::ram::RamFileSystem;

    use super::*;

    #[test]
    fn test_write_read_file() {
        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from("sub"));
        super::super::tests::test_write_read_file(fs);

        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from("sub/sub"));
        super::super::tests::test_write_read_file(fs);

        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from(""));
        super::super::tests::test_write_read_file(fs);
    }

    #[test]
    fn test_list() {
        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from("sub"));
        super::super::tests::test_list(fs);

        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from("sub/sub"));
        super::super::tests::test_list(fs);

        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from(""));
        super::super::tests::test_list(fs);
    }

    #[test]
    fn test_delete() {
        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from("sub"));
        super::super::tests::test_delete(fs);

        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from("sub/sub"));
        super::super::tests::test_delete(fs);

        let ram = RamFileSystem::new();
        let fs = ScopedFileSystem::new(ram, PathBuf::from(""));
        super::super::tests::test_delete(fs);
    }
}
