use std::{
    collections::BTreeMap,
    io::{Read, Seek, SeekFrom, Write},
    sync::{Arc, RwLock},
};

use super::*;

pub struct RamFileSystem {
    files: Arc<RwLock<BTreeMap<PathBuf, RamFileData>>>,
}

#[derive(Clone, Default)]
struct RamFileData {
    bytes: Arc<RwLock<Vec<u8>>>,
}

impl RamFileSystem {
    pub fn new() -> Self {
        RamFileSystem {
            files: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }
}

impl Default for RamFileSystem {
    fn default() -> Self {
        Self::new()
    }
}

impl FileSystem for RamFileSystem {
    fn open_read(&self, path: &std::path::Path) -> Result<Box<dyn FileRead>, Error> {
        if !path.has_root() || path.parent().is_none() {
            return Err(Error::InvalidFilePathRoot);
        }

        let files = self.files.read().unwrap();
        let file = files
            .get(path)
            .ok_or_else(|| Error::NotFound(path.to_path_buf()))?;

        Ok(Box::new(RamFile {
            data: file.clone(),
            cursor: 0,
        }))
    }

    fn open_write(&self, path: &Path) -> Result<Box<dyn FileWrite>, Error> {
        if !path.has_root() || path.parent().is_none() {
            return Err(Error::InvalidFilePathRoot);
        }

        let mut files = self.files.write().unwrap();

        let data = files.get(path).cloned();
        let data = if let Some(data) = data {
            data
        } else {
            let data = RamFileData::default();
            files.insert(path.to_path_buf(), data.clone());
            data
        };

        Ok(Box::new(RamFile { data, cursor: 0 }))
    }

    fn list(&self, prefix: Option<&Path>) -> Result<Vec<Box<dyn FileStat>>, Error> {
        let prefix = prefix.unwrap_or_else(|| Path::new("/"));
        let files = self.files.read().unwrap();

        let mut result: Vec<Box<dyn FileStat>> = Vec::new();
        for (path, file) in files.iter() {
            if path.starts_with(prefix) {
                result.push(Box::new(RamFileStat {
                    path: path.to_path_buf(),
                    data: file.clone(),
                }));
            }
        }

        Ok(result)
    }

    fn stat(&self, path: &Path) -> Result<Box<dyn FileStat>, Error> {
        let files = self.files.read().unwrap();
        let file = files
            .get(path)
            .ok_or_else(|| Error::NotFound(path.to_path_buf()))?;

        Ok(Box::new(RamFileStat {
            path: path.to_path_buf(),
            data: file.clone(),
        }))
    }

    fn exists(&self, path: &Path) -> bool {
        let files = self.files.read().unwrap();
        files.contains_key(path)
    }

    fn delete(&self, path: &Path) -> Result<(), Error> {
        let mut files = self.files.write().unwrap();
        files.remove(path);
        Ok(())
    }

    fn clear(&self) -> Result<(), Error> {
        let mut files = self.files.write().unwrap();
        files.clear();
        Ok(())
    }

    fn clone(&self) -> DynFileSystem {
        DynFileSystem(Box::new(RamFileSystem {
            files: self.files.clone(),
        }))
    }

    fn as_os_path(&self, _path: &Path) -> Result<PathBuf, Error> {
        Err(Error::NotOsPath)
    }
}

pub struct RamFile {
    data: RamFileData,
    cursor: usize,
}

impl FileRead for RamFile {}
impl FileWrite for RamFile {}

impl Read for RamFile {
    fn read(&mut self, buf: &mut [u8]) -> std::io::Result<usize> {
        let data = self.data.bytes.read().unwrap();
        if self.cursor >= data.len() {
            return Ok(0);
        }

        let to = std::cmp::min(self.cursor + buf.len(), data.len());
        let count = (&data[self.cursor..to]).read(buf)?;
        self.cursor += count;
        Ok(count)
    }
}

impl Seek for RamFile {
    fn seek(&mut self, pos: SeekFrom) -> std::io::Result<u64> {
        let data = self.data.bytes.read().unwrap();
        let len = data.len();
        let pos = match pos {
            SeekFrom::Start(pos) => pos as u64,
            SeekFrom::End(pos) => (len as i64 + pos) as u64,
            SeekFrom::Current(pos) => (self.cursor as i64 + pos) as u64,
        };
        self.cursor = pos as usize;
        Ok(pos)
    }
}

impl Write for RamFile {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let mut data = self.data.bytes.write().unwrap();

        for i in 0..buf.len() {
            if self.cursor + i >= data.len() {
                data.push(buf[i]);
            } else {
                data[self.cursor + i] = buf[i];
            }
        }

        self.cursor += buf.len();

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct RamFileStat {
    path: PathBuf,
    data: RamFileData,
}

impl FileStat for RamFileStat {
    fn path(&self) -> &Path {
        self.path.as_path()
    }

    fn size(&self) -> u64 {
        self.data.bytes.read().unwrap().len() as u64
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_read_file() {
        let fs = RamFileSystem::new();

        {
            // cannot create at root
            assert!(fs.open_write(Path::new("/")).is_err());
        }

        {
            // can create file
            assert!(!fs.exists(Path::new("/file1")));

            let mut file = fs.open_write(Path::new("/file1")).unwrap();
            file.write_all(b"Hello ").unwrap();
            file.write_all(b"world").unwrap();

            let stat = fs.stat(Path::new("/file1")).unwrap();
            assert_eq!(stat.path(), Path::new("/file1"));
            assert_eq!(stat.size(), 11);

            assert!(fs.exists(Path::new("/file1")));
        }

        {
            // can read the file
            let mut file = fs.open_read(Path::new("/file1")).unwrap();
            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("Hello world", buf);

            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("", buf);
        }

        {
            // can seek
            let mut file = fs.open_write(Path::new("/file1")).unwrap();

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
    }

    #[test]
    fn test_list() {
        let fs = RamFileSystem::new();
        assert!(fs.list(None).unwrap().is_empty());
        assert!(fs.list(Some(Path::new("/"))).unwrap().is_empty());

        {
            fs.open_write(Path::new("/dir1/file1")).unwrap();
            fs.open_write(Path::new("/dir1/file2")).unwrap();
            fs.open_write(Path::new("/dir1/file3")).unwrap();
            fs.open_write(Path::new("/dir2/file1")).unwrap();
            fs.open_write(Path::new("/dir2/file2")).unwrap();
            fs.open_write(Path::new("/file1")).unwrap();
        }

        assert_eq!(fs.list(Some(Path::new("/dir1"))).unwrap().len(), 3);
        assert_eq!(fs.list(Some(Path::new("/dir2"))).unwrap().len(), 2);
        assert_eq!(fs.list(Some(Path::new("/file1"))).unwrap().len(), 1);
        assert_eq!(fs.list(Some(Path::new("/"))).unwrap().len(), 6);
        assert_eq!(fs.list(Some(Path::new("/not/found"))).unwrap().len(), 0);
    }

    #[test]
    fn test_clear() {
        let fs = RamFileSystem::new();

        {
            let mut file = fs.open_write(Path::new("/test")).unwrap();
            file.write_all(b"Hello").unwrap();
        }

        assert!(fs.exists(Path::new("/test")));

        fs.clear().unwrap();

        assert!(!fs.exists(Path::new("/test")));
    }

    #[test]
    fn test_delete() {
        let fs = RamFileSystem::new();

        {
            let mut file = fs.open_write(Path::new("/test")).unwrap();
            file.write_all(b"Hello").unwrap();
        }

        assert!(fs.exists(Path::new("/test")));

        fs.delete(Path::new("/test")).unwrap();

        assert!(!fs.exists(Path::new("/test")));
    }
}
