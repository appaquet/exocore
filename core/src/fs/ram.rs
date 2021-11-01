use std::{
    collections::{BTreeMap, HashMap},
    io::{Cursor, Read, Seek, SeekFrom, Write},
    sync::{Arc, RwLock, Weak},
};

use bytes::{Buf, Bytes, BytesMut};

use super::*;

pub struct RamFileSystem {
    files: Arc<RwLock<BTreeMap<Path, RamFileData>>>,
}

#[derive(Clone, Default)]
struct RamFileData {
    data: Arc<RwLock<Vec<u8>>>,
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
    fn open_read(&self, path: &Path) -> Result<Box<dyn FileRead>, Error> {
        if path.is_root() {
            return Err(Error::InvalidFilePathRoot);
        }

        let files = self.files.read().unwrap();
        let file = files
            .get(path)
            .ok_or_else(|| Error::NotFound(path.clone()))?;

        Ok(Box::new(RamFile {
            data: file.clone(),
            cursor: 0,
        }))
    }

    fn open_write(&self, path: &Path) -> Result<Box<dyn FileWrite>, Error> {
        if path.is_root() {
            return Err(Error::InvalidFilePathRoot);
        }

        let mut files = self.files.write().unwrap();

        let data = files.get(path).cloned();
        let data = if let Some(data) = data {
            data
        } else {
            let data = RamFileData::default();
            files.insert(path.clone(), data.clone());
            data
        };

        Ok(Box::new(RamFile { data, cursor: 0 }))
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
        let data = self.data.data.read().unwrap();
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
        let data = self.data.data.read().unwrap();
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
        let mut data = self.data.data.write().unwrap();

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_write_file() {
        let fs = RamFileSystem::new();

        {
            // cannot create at root
            assert!(fs.open_write(&"/".into()).is_err());
        }

        {
            // can create file
            assert!(!fs.exists(&"/file1".into()));

            let mut file = fs.open_write(&"/file1".into()).unwrap();
            file.write_all(b"Hello ").unwrap();
            file.write_all(b"world").unwrap();

            assert!(fs.exists(&"/file1".into()));
        }

        {
            // can read the file
            let mut file = fs.open_read(&"/file1".into()).unwrap();
            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("Hello world", buf);

            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("", buf);
        }

        {
            // can seek
            let mut file = fs.open_write(&"/file1".into()).unwrap();

            file.seek(SeekFrom::Start(6)).unwrap();
            file.write_all(b"monde").unwrap();

            let mut buf = String::new();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("", buf);

            file.seek( SeekFrom::Start(0)).unwrap();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("Hello monde", buf);

            file.seek( SeekFrom::End(-5)).unwrap();
            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("monde", buf);

            file.seek( SeekFrom::Current(-5)).unwrap();
            buf.clear();
            file.read_to_string(&mut buf).unwrap();
            assert_eq!("monde", buf);
        }
    }
}
