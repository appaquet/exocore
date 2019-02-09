use std;
use std::cmp::Ordering;
use std::fs::{File, OpenOptions};
use std::path::{Path, PathBuf};

use exocore_common::data_chain_capnp::{block, block_signatures};
use exocore_common::range;
use exocore_common::serialization::msg;
use exocore_common::serialization::msg::{
    FramedMessage, FramedMessageIterator, FramedTypedMessage, MessageType,
};

use super::*;

// TODO: Snappy compression? Is it even worth it since the data is encrypted?
// TODO: Opening segments could be faster to open by passing last known offset
// TODO: Caching of segments metadata
// TODO: Segments hash & sign hashes using in-memory key ==> Makes sure that nobody changed the file while we were offline

///
/// Directory persistence
///
#[derive(Copy, Clone, Debug)]
struct DirectoryConfig {
    segment_over_allocate_size: u64,
    segment_min_free_size: u64,
    segment_max_size: u64,
}

impl Default for DirectoryConfig {
    fn default() -> Self {
        DirectoryConfig {
            segment_over_allocate_size: 300 * 1024 * 1024, // 300mb
            segment_min_free_size: 10 * 1024 * 1024,       // 10mb
            segment_max_size: 4 * 1024 * 1024 * 1024,      // 4gb
        }
    }
}

struct DirectoryStore {
    config: DirectoryConfig,
    directory: PathBuf,
    segments: Vec<DirectorySegment>,
}

impl DirectoryStore {
    fn create(config: DirectoryConfig, directory_path: &Path) -> Result<DirectoryStore, Error> {
        if !directory_path.exists() {
            error!(
                "Tried to create directory at {:?}, but it didn't exist",
                directory_path
            );
            return Err(Error::UnexpectedState);
        }

        let paths = std::fs::read_dir(directory_path).map_err(|err| {
            error!("Error listing directory {:?}: {:?}", directory_path, err);
            Error::IO
        })?;

        if paths.count() > 0 {
            error!(
                "Tried to create directory at {:?}, but it's not empty",
                directory_path
            );
            return Err(Error::UnexpectedState);
        }

        Ok(DirectoryStore {
            config,
            directory: directory_path.to_path_buf(),
            segments: Vec::new(),
        })
    }

    fn open(config: DirectoryConfig, directory_path: &Path) -> Result<DirectoryStore, Error> {
        if !directory_path.exists() {
            error!(
                "Tried to open directory at {:?}, but it didn't exist",
                directory_path
            );
            return Err(Error::UnexpectedState);
        }

        let mut segments = Vec::new();
        let paths = std::fs::read_dir(directory_path).map_err(|err| {
            error!("Error listing directory {:?}: {:?}", directory_path, err);
            Error::IO
        })?;
        for path in paths {
            let path = path.map_err(|err| {
                error!("Error getting directory entry {:?}", err);
                Error::IO
            })?;

            let segment = DirectorySegment::open(config, &path.path())?;
            segments.push(segment);
        }
        segments.sort_by(|a, b| a.first_block_offset.cmp(&b.first_block_offset));

        Ok(DirectoryStore {
            config,
            directory: directory_path.to_path_buf(),
            segments,
        })
    }

    fn get_segment_index_for_block_offset(&self, block_offset: BlockOffset) -> Option<usize> {
        self.segments
            .binary_search_by(|seg| {
                if block_offset >= seg.first_block_offset && block_offset < seg.next_block_offset {
                    Ordering::Equal
                } else if block_offset < seg.first_block_offset {
                    Ordering::Greater
                } else {
                    Ordering::Less
                }
            })
            .ok()
    }

    fn get_segment_for_block_offset(&self, block_offset: BlockOffset) -> Option<&DirectorySegment> {
        let segment_index = self.get_segment_index_for_block_offset(block_offset)?;
        self.segments.get(segment_index)
    }

    fn get_segment_for_next_block_offset(
        &self,
        next_block_offset: BlockOffset,
    ) -> Option<&DirectorySegment> {
        if next_block_offset > 0 {
            self.get_segment_for_block_offset(next_block_offset - 1)
        } else {
            None
        }
    }
}

impl Store for DirectoryStore {
    fn write_block<B, S>(&mut self, block: &B, block_signatures: &S) -> Result<BlockOffset, Error>
    where
        B: msg::FramedTypedMessage<block::Owned>,
        S: msg::FramedTypedMessage<block_signatures::Owned>,
    {
        let (block_segment, written_in_segment) = {
            let need_new_segment = {
                match self.segments.last() {
                    None => true,
                    Some(s) => s.next_file_offset as u64 > self.config.segment_max_size,
                }
            };

            if need_new_segment {
                let segment = DirectorySegment::create(
                    self.config,
                    &self.directory,
                    block,
                    block_signatures,
                )?;
                self.segments.push(segment);
            }

            (self.segments.last_mut().unwrap(), need_new_segment)
        };

        // when creating new segment, blocks get written right away
        if !written_in_segment {
            block_segment.write_block(block, block_signatures)?;
        }

        Ok(block_segment.next_block_offset)
    }

    fn available_segments(&self) -> Vec<range::Range<BlockOffset>> {
        self.segments
            .iter()
            .map(|segment| segment.offset_range())
            .collect()
    }

    fn block_iter(&self, from_offset: BlockOffset) -> Result<StoredBlockIterator, Error> {
        Ok(Box::new(DirectoryBlockIterator {
            directory: self,
            current_offset: from_offset,
            current_segment: None,
            last_error: None,
            reverse: false,
            done: false,
        }))
    }

    fn block_iter_reverse(
        &self,
        from_next_offset: BlockOffset,
    ) -> Result<StoredBlockIterator, Error> {
        let segment = self
            .get_segment_for_next_block_offset(from_next_offset)
            .ok_or(Error::OutOfBound)?;

        let last_block = segment.get_block_from_next_offset(from_next_offset)?;

        Ok(Box::new(DirectoryBlockIterator {
            directory: self,
            current_offset: last_block.get_offset()?,
            current_segment: None,
            last_error: None,
            reverse: true,
            done: false,
        }))
    }

    fn get_block(&self, offset: BlockOffset) -> Result<StoredBlock, Error> {
        let segment = self
            .get_segment_for_block_offset(offset)
            .ok_or(Error::OutOfBound)?;

        segment.get_block(offset)
    }

    fn get_block_from_next_offset(&self, next_offset: BlockOffset) -> Result<StoredBlock, Error> {
        let segment = self
            .get_segment_for_next_block_offset(next_offset)
            .ok_or(Error::OutOfBound)?;

        segment.get_block_from_next_offset(next_offset)
    }

    fn truncate_from_offset(&mut self, block_offset: BlockOffset) -> Result<(), Error> {
        let segment_index = self
            .get_segment_index_for_block_offset(block_offset)
            .ok_or(Error::OutOfBound)?;

        let truncate_to = {
            let segment = &mut self.segments[segment_index];
            if block_offset > segment.first_block_offset {
                segment.truncate_from_block_offset(block_offset)?;
                segment_index + 1
            } else {
                segment_index
            }
        };

        if truncate_to < self.segments.len() {
            let removed_segments = self.segments.split_off(truncate_to);
            for segment in removed_segments {
                segment.delete()?;
            }
        }

        Ok(())
    }
}

struct DirectoryBlockIterator<'pers> {
    directory: &'pers DirectoryStore,
    current_offset: BlockOffset,
    current_segment: Option<&'pers DirectorySegment>,
    last_error: Option<Error>,
    reverse: bool,
    done: bool,
}

impl<'pers> Iterator for DirectoryBlockIterator<'pers> {
    type Item = StoredBlock<'pers>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.done {
            return None;
        }

        if self.current_segment.is_none() {
            self.current_segment = self
                .directory
                .get_segment_for_block_offset(self.current_offset);
        }

        let (item, data_size, end_of_segment) = match self.current_segment {
            Some(segment) => {
                let block = segment
                    .get_block(self.current_offset)
                    .map_err(|err| {
                        error!("Got an error getting block in iterator: {:?}", err);
                        self.last_error = Some(err);
                    })
                    .ok()?;

                let data_size = block.total_size() as BlockOffset;
                let end_of_segment = if !self.reverse {
                    (self.current_offset + data_size) >= segment.next_block_offset
                } else {
                    data_size > self.current_offset
                        || (self.current_offset - data_size) < segment.first_block_offset
                };

                (block, data_size, end_of_segment)
            }
            None => {
                return None;
            }
        };

        if end_of_segment {
            self.current_segment = None;
        }

        // if we're in reverse and next offset would be lower than 0, we indicate we're done
        if self.reverse && data_size > self.current_offset {
            self.done = true;
        }

        if !self.done {
            if !self.reverse {
                self.current_offset += data_size;
            } else {
                self.current_offset -= data_size;
            }
        }

        Some(item)
    }
}

struct DirectorySegment {
    config: DirectoryConfig,
    first_block_offset: BlockOffset,
    segment_path: PathBuf,
    segment_file: SegmentFile,
    last_block_offset: BlockOffset,
    next_block_offset: BlockOffset,
    next_file_offset: usize,
}

impl DirectorySegment {
    fn create<B, S>(
        config: DirectoryConfig,
        directory: &Path,
        block: &B,
        block_sigs: &S,
    ) -> Result<DirectorySegment, Error>
    where
        B: msg::FramedTypedMessage<block::Owned>,
        S: msg::FramedTypedMessage<block_signatures::Owned>,
    {
        let block_reader = block.get_typed_reader().unwrap();
        let first_block_offset = block_reader.get_offset();
        let last_block_offset = first_block_offset;

        let segment_path = Self::segment_path(directory, first_block_offset);
        if segment_path.exists() {
            error!(
                "Tried to create a new segment at path {:?}, but already existed",
                segment_path
            );
            return Err(Error::UnexpectedState);
        }

        info!(
            "Creating new segment at {:?} for offset {}",
            directory, first_block_offset
        );
        let mut segment_file = SegmentFile::open(&segment_path, config.segment_over_allocate_size)?;
        block.copy_into(&mut segment_file.mmap);
        block_sigs.copy_into(&mut segment_file.mmap[block.frame_size()..]);
        let written_data_size = block.frame_size() + block_sigs.frame_size();

        Ok(DirectorySegment {
            config,
            first_block_offset,
            segment_path,
            segment_file,
            last_block_offset,
            next_block_offset: first_block_offset + written_data_size as BlockOffset,
            next_file_offset: written_data_size,
        })
    }

    fn open_with_first_offset(
        config: DirectoryConfig,
        directory: &Path,
        first_offset: BlockOffset,
    ) -> Result<DirectorySegment, Error> {
        let segment_path = Self::segment_path(directory, first_offset);
        let segment = Self::open(config, &segment_path)?;

        if segment.first_block_offset != first_offset {
            error!(
                "First block offset != segment first_offset ({} != {})",
                segment.first_block_offset, first_offset
            );
            return Err(Error::Integrity);
        }

        Ok(segment)
    }

    fn open(config: DirectoryConfig, segment_path: &Path) -> Result<DirectorySegment, Error> {
        info!("Opening segment at {:?}", segment_path);

        let segment_file = SegmentFile::open(&segment_path, 0)?;

        // read first block to validate it has the same offset as segment
        let first_block_offset = {
            let framed_message =
                msg::FramedSliceMessage::new(&segment_file.mmap).map_err(|err| {
                    error!(
                        "Couldn't read first block from segment file {:?}: {:?}",
                        segment_path, err
                    );
                    err
                })?;
            let first_block = framed_message.get_typed_reader::<block::Owned>()?;
            first_block.get_offset()
        };

        // iterate through segments and find the last block and its offset
        let (last_block_offset, next_block_offset, next_file_offset) = {
            let mut last_block_file_offset = None;
            let block_iter = FramedMessageIterator::new(&segment_file.mmap)
                .filter(|msg| msg.framed_message.message_type() == block::Owned::message_type());
            for message in block_iter {
                last_block_file_offset = Some(message.offset);
            }

            match last_block_file_offset {
                Some(file_offset) => {
                    let block_message =
                        msg::FramedSliceMessage::new(&segment_file.mmap[file_offset..])?;
                    let block_reader = block_message.get_typed_reader::<block::Owned>()?;
                    let sigs_offset = file_offset + block_message.frame_size();
                    let sigs_message =
                        msg::FramedSliceMessage::new(&segment_file.mmap[sigs_offset..])?;

                    let written_data_size = block_message.frame_size() + sigs_message.frame_size();
                    (
                        block_reader.get_offset(),
                        block_reader.get_offset() + written_data_size as BlockOffset,
                        file_offset + written_data_size,
                    )
                }
                _ => {
                    error!("Couldn't find last block of segment: no blocks returned by iterator");
                    return Err(Error::Integrity);
                }
            }
        };

        Ok(DirectorySegment {
            config,
            first_block_offset,
            segment_path: segment_path.to_path_buf(),
            segment_file,
            last_block_offset,
            next_block_offset,
            next_file_offset,
        })
    }

    fn segment_path(directory: &Path, first_offset: BlockOffset) -> PathBuf {
        directory.join(format!("seg_{}", first_offset))
    }

    fn offset_range(&self) -> range::Range<BlockOffset> {
        range::Range::new(self.first_block_offset, self.next_block_offset)
    }

    fn ensure_file_size(&mut self, write_size: usize) -> Result<(), Error> {
        let next_file_offset = self.next_file_offset;

        if self.segment_file.current_size < (next_file_offset + write_size) as u64 {
            let target_size =
                (next_file_offset + write_size) as u64 + self.config.segment_over_allocate_size;
            if target_size > self.config.segment_max_size {
                return Err(Error::SegmentFull);
            }

            self.segment_file.set_len(target_size)?;
        }

        Ok(())
    }

    fn write_block<B, S>(&mut self, block: &B, block_sigs: &S) -> Result<(), Error>
    where
        B: msg::FramedTypedMessage<block::Owned>,
        S: msg::FramedTypedMessage<block_signatures::Owned>,
    {
        let next_file_offset = self.next_file_offset;
        let next_block_offset = self.next_block_offset;
        let block_size = block.frame_size();
        let sigs_size = block_sigs.frame_size();

        let block_reader = block.get_typed_reader()?;
        let block_offset = block_reader.get_offset();
        if next_block_offset != block_offset {
            error!("Trying to write a block at an offset that wasn't next offset: next_block_offset={} block_offset={}", next_block_offset, block_offset);
            return Err(Error::Integrity);
        }

        {
            self.ensure_file_size(block_size + sigs_size)?;
            block.copy_into(&mut self.segment_file.mmap[next_file_offset..]);
            block_sigs.copy_into(&mut self.segment_file.mmap[next_file_offset + block_size..]);
        }

        self.next_file_offset += block_size + sigs_size;
        self.next_block_offset += (block_size + sigs_size) as BlockOffset;

        Ok(())
    }

    fn get_block(&self, offset: BlockOffset) -> Result<StoredBlock, Error> {
        let first_block_offset = self.first_block_offset;
        if offset < first_block_offset {
            error!(
                "Tried to read block at {}, but first offset was at {}",
                offset, first_block_offset
            );
            return Err(Error::OutOfBound);
        }

        if offset >= self.next_block_offset {
            error!(
                "Tried to read block at {}, but next offset was at {}",
                offset, self.next_block_offset
            );
            return Err(Error::OutOfBound);
        }

        let block_file_offset = (offset - first_block_offset) as usize;
        let block =
            msg::FramedSliceTypedMessage::new(&self.segment_file.mmap[block_file_offset..])?;

        let signatures_file_offset = block_file_offset + block.frame_size();
        let signatures =
            msg::FramedSliceTypedMessage::new(&self.segment_file.mmap[signatures_file_offset..])?;

        Ok(StoredBlock { block, signatures })
    }

    fn get_block_from_next_offset(&self, next_offset: BlockOffset) -> Result<StoredBlock, Error> {
        let first_block_offset = self.first_block_offset;
        if next_offset < first_block_offset {
            error!(
                "Tried to read block from next offset {}, but first offset was at {}",
                next_offset, first_block_offset
            );
            return Err(Error::OutOfBound);
        }

        if next_offset > self.next_block_offset {
            error!(
                "Tried to read block from next offset {}, but next offset was at {}",
                next_offset, self.next_block_offset
            );
            return Err(Error::OutOfBound);
        }

        let next_file_offset = (next_offset - first_block_offset) as usize;
        let signatures = msg::FramedSliceTypedMessage::new_from_next_offset(
            &self.segment_file.mmap[..],
            next_file_offset,
        )?;
        let signatures_offset = next_file_offset - signatures.frame_size();

        let block = msg::FramedSliceTypedMessage::new_from_next_offset(
            &self.segment_file.mmap[..],
            signatures_offset,
        )?;

        Ok(StoredBlock { block, signatures })
    }

    fn truncate_extra(&mut self) -> Result<(), Error> {
        let next_file_offset = self.next_file_offset as u64;
        self.segment_file.set_len(next_file_offset)
    }

    fn truncate_from_block_offset(&mut self, block_offset: BlockOffset) -> Result<(), Error> {
        if block_offset < self.first_block_offset {
            return Err(Error::OutOfBound);
        }
        self.next_block_offset = block_offset;
        let keep_len = block_offset - self.first_block_offset;
        self.segment_file.set_len(keep_len)
    }

    fn delete(self) -> Result<(), Error> {
        let segment_path = self.segment_path.clone();
        drop(self);
        std::fs::remove_file(&segment_path).map_err(|err| {
            error!("Couldn't delete segment file {:?}: {:?}", segment_path, err);
            Error::IO
        })?;
        Ok(())
    }
}

struct SegmentFile {
    path: PathBuf,
    file: File,
    mmap: memmap::MmapMut,
    current_size: u64,
}

impl SegmentFile {
    fn open(path: &Path, minimum_size: u64) -> Result<SegmentFile, Error> {
        let file = OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .open(path)
            .map_err(|err| {
                error!("Error opening/creating segment file {:?}: {:?}", path, err);
                Error::IO
            })?;

        let mut current_size = std::fs::metadata(path).map(|m| m.len()).unwrap_or(0);
        if current_size < minimum_size {
            current_size = minimum_size;
            file.set_len(minimum_size).map_err(|err| {
                error!("Error setting len of segment file {:?}: {:?}", path, err);
                Error::IO
            })?;
        }

        let mmap = unsafe {
            memmap::MmapOptions::new().map_mut(&file).map_err(|err| {
                error!("Error mmaping segment file {:?}: {:?}", path, err);
                Error::IO
            })?
        };

        Ok(SegmentFile {
            path: path.to_path_buf(),
            file,
            mmap,
            current_size,
        })
    }

    fn set_len(&mut self, new_size: u64) -> Result<(), Error> {
        self.file.set_len(new_size).map_err(|err| {
            error!(
                "Error setting len of segment file {:?}: {:?}",
                self.path, err
            );
            Error::IO
        })?;

        self.mmap = unsafe {
            memmap::MmapOptions::new()
                .map_mut(&self.file)
                .map_err(|err| {
                    error!("Error mmaping segment file {:?}: {:?}", self.path, err);
                    Error::IO
                })?
        };

        self.current_size = new_size;
        Ok(())
    }
}

impl From<msg::Error> for Error {
    fn from(err: msg::Error) -> Self {
        Error::Serialization(err)
    }
}

#[cfg(test)]
mod tests {
    use tempdir;

    use super::*;
    use exocore_common::serialization::msg::{FramedOwnedTypedMessage, FramedTypedMessage};

    #[test]
    fn directory_chain_create_and_open() {
        let dir = tempdir::TempDir::new("test").unwrap();
        let config: DirectoryConfig = Default::default();

        let init_segments = {
            let mut directory_chain = DirectoryStore::create(config, dir.path()).unwrap();

            let block_msg = create_block(0);
            let sig_msg = create_block_sigs();
            let second_offset = directory_chain.write_block(&block_msg, &sig_msg).unwrap();

            let block = directory_chain.get_block(0).unwrap();
            assert_eq!(block.get_offset().unwrap(), 0);
            let block = directory_chain
                .get_block_from_next_offset(second_offset)
                .unwrap();
            assert_eq!(block.get_offset().unwrap(), 0);

            let block_msg = create_block(second_offset);
            let sig_msg = create_block_sigs();
            let third_offset = directory_chain.write_block(&block_msg, &sig_msg).unwrap();
            let block = directory_chain.get_block(second_offset).unwrap();
            assert_eq!(block.get_offset().unwrap(), second_offset);
            let block = directory_chain
                .get_block_from_next_offset(third_offset)
                .unwrap();
            assert_eq!(block.get_offset().unwrap(), second_offset);

            let segments = directory_chain.available_segments();
            let data_size = ((block_msg.frame_size() + sig_msg.frame_size()) * 2) as BlockOffset;
            assert_eq!(segments, vec![range::Range::new(0, data_size)]);
            segments
        };

        {
            // already exists
            assert!(DirectoryStore::create(config, dir.path()).is_err());
        }

        {
            let directory_chain = DirectoryStore::open(config, dir.path()).unwrap();
            assert_eq!(directory_chain.available_segments(), init_segments);
        }
    }

    #[test]
    fn directory_chain_write_until_second_segment() {
        let dir = tempdir::TempDir::new("test").unwrap();
        let mut config: DirectoryConfig = Default::default();
        config.segment_max_size = 100_000;

        fn validate_directory(directory_chain: &DirectoryStore) {
            let segments = directory_chain.available_segments();
            assert!(range::are_continuous(segments.iter()));
            assert_eq!(segments.len(), 2);

            let block = directory_chain.get_block(0).unwrap();
            assert_eq!(block.get_offset().unwrap(), 0);

            let block = directory_chain.get_block(segments[0].to).unwrap();
            assert_eq!(block.get_offset().unwrap(), segments[0].to);

            let block = directory_chain
                .get_block_from_next_offset(segments[0].to)
                .unwrap();
            assert_eq!(block.next_offset().unwrap(), segments[0].to);

            let block = directory_chain
                .get_block_from_next_offset(segments[0].to)
                .unwrap();
            assert_eq!(block.next_offset().unwrap(), segments[0].to);

            let last_block = directory_chain
                .get_block_from_next_offset(segments[1].to)
                .unwrap();

            let last_block_offset = last_block.get_offset().unwrap();
            let next_block_offset = last_block.next_offset().unwrap();
            assert_eq!(next_block_offset, segments[1].to);

            // validate data using forward and reverse iterators
            let mut iterator = directory_chain.block_iter(0).unwrap();
            validate_iterator(iterator, 1000, 0, last_block_offset, false);

            let next_block_offset = segments.last().unwrap().to;
            let mut reverse_iterator = directory_chain
                .block_iter_reverse(next_block_offset)
                .unwrap();
            validate_iterator(reverse_iterator, 1000, last_block_offset, 0, true);
        }

        let init_segments = {
            let mut directory_chain = DirectoryStore::create(config, dir.path()).unwrap();

            append_blocks_to_directory(&mut directory_chain, 1000, 0);
            validate_directory(&directory_chain);

            directory_chain.available_segments()
        };

        {
            let directory_chain = DirectoryStore::open(config, dir.path()).unwrap();
            assert_eq!(directory_chain.available_segments(), init_segments);

            validate_directory(&directory_chain);
        }
    }

    #[test]
    fn directory_chain_truncate() {
        let mut config: DirectoryConfig = Default::default();
        config.segment_max_size = 1000;

        // we cutoff the directory at different position to make sure of its integrity
        for cutoff in 1..50 {
            let dir = tempdir::TempDir::new("test").unwrap();

            let (segments_before, block_n_offset, block_n_plus_offset) = {
                let mut directory_chain = DirectoryStore::create(config, dir.path()).unwrap();
                append_blocks_to_directory(&mut directory_chain, 50, 0);
                let segments_before = directory_chain.available_segments();

                let block_n = directory_chain
                    .block_iter(0)
                    .unwrap()
                    .skip(cutoff - 1)
                    .next()
                    .unwrap();
                let block_n_offset = block_n.get_offset().unwrap();
                let block_n_plus_offset = block_n.next_offset().unwrap();

                directory_chain
                    .truncate_from_offset(block_n_plus_offset)
                    .unwrap();

                let segments_after = directory_chain.available_segments();
                assert_ne!(segments_before, segments_after);
                assert_eq!(segments_after.last().unwrap().to, block_n_plus_offset);

                let mut iter = directory_chain.block_iter(0).unwrap();
                validate_iterator(iter, cutoff, 0, block_n_offset, false);

                let mut iter_reverse = directory_chain
                    .block_iter_reverse(block_n_plus_offset)
                    .unwrap();
                validate_iterator(iter_reverse, cutoff, block_n_offset, 0, true);

                (segments_before, block_n_offset, block_n_plus_offset)
            };

            {
                let mut directory_chain = DirectoryStore::open(config, dir.path()).unwrap();

                let segments_after = directory_chain.available_segments();
                assert_ne!(segments_before, segments_after);
                assert_eq!(segments_after.last().unwrap().to, block_n_plus_offset);

                let mut iter = directory_chain.block_iter(0).unwrap();
                validate_iterator(iter, cutoff, 0, block_n_offset, false);

                let mut iter_reverse = directory_chain
                    .block_iter_reverse(block_n_plus_offset)
                    .unwrap();
                validate_iterator(iter_reverse, cutoff, block_n_offset, 0, true);
            }
        }
    }

    #[test]
    fn directory_chain_truncate_all() {
        let mut config: DirectoryConfig = Default::default();
        config.segment_max_size = 3000;
        let dir = tempdir::TempDir::new("test").unwrap();

        {
            let mut directory_chain = DirectoryStore::create(config, dir.path()).unwrap();
            append_blocks_to_directory(&mut directory_chain, 100, 0);

            directory_chain.truncate_from_offset(0).unwrap();

            let segments_after = directory_chain.available_segments();
            assert!(segments_after.is_empty());
        }

        {
            let mut directory_chain = DirectoryStore::open(config, dir.path()).unwrap();
            let segments = directory_chain.available_segments();
            assert!(segments.is_empty());
        }
    }

    #[test]
    fn directory_segment_create_and_open() {
        let dir = tempdir::TempDir::new("test").unwrap();

        let segment_id = 1234;
        let block_msg = create_block(1234);
        let sig_msg = create_block_sigs();

        {
            let segment =
                DirectorySegment::create(Default::default(), dir.path(), &block_msg, &sig_msg)
                    .unwrap();
            assert_eq!(segment.first_block_offset, 1234);
            assert_eq!(segment.last_block_offset, 1234);
            assert_eq!(
                segment.next_file_offset,
                block_msg.frame_size() + sig_msg.frame_size()
            );
            assert_eq!(
                segment.next_block_offset,
                1234 + (block_msg.frame_size() + sig_msg.frame_size()) as BlockOffset
            );
        }

        {
            let segment = DirectorySegment::open_with_first_offset(
                Default::default(),
                dir.path(),
                segment_id,
            )
            .unwrap();
            assert_eq!(segment.first_block_offset, 1234);
            assert_eq!(segment.last_block_offset, 1234);
            assert_eq!(
                segment.next_file_offset,
                block_msg.frame_size() + sig_msg.frame_size()
            );
            assert_eq!(
                segment.next_block_offset,
                1234 + (block_msg.frame_size() + sig_msg.frame_size()) as BlockOffset
            );
        }
    }

    #[test]
    fn directory_segment_create_already_exist() {
        let dir = tempdir::TempDir::new("test").unwrap();

        {
            let block_msg = create_block(1234);
            let sig_msg = create_block_sigs();
            let _segment =
                DirectorySegment::create(Default::default(), dir.path(), &block_msg, &sig_msg)
                    .unwrap();
        }

        {
            let block_msg = create_block(1234);
            let sig_msg = create_block_sigs();
            assert!(
                DirectorySegment::create(Default::default(), dir.path(), &block_msg, &sig_msg)
                    .is_err()
            );
        }
    }

    #[test]
    fn directory_segment_append_block() {
        let dir = tempdir::TempDir::new("test").unwrap();

        let offset1 = 0;
        let block = create_block(offset1);
        let block_sigs = create_block_sigs();
        let mut segment =
            DirectorySegment::create(Default::default(), dir.path(), &block, &block_sigs).unwrap();
        {
            let block = segment.get_block(offset1).unwrap();
            assert_eq!(block.get_offset().unwrap(), offset1);
        }

        let offset2 = offset1 + (block.frame_size() + block_sigs.frame_size()) as u64;
        assert_eq!(segment.next_block_offset, offset2);
        let block = create_block(offset2);
        let block_sigs = create_block_sigs();
        segment.write_block(&block, &block_sigs).unwrap();
        {
            let block = segment.get_block(offset2).unwrap();
            assert_eq!(block.get_offset().unwrap(), offset2);
        }

        let offset3 = offset2 + (block.frame_size() + block_sigs.frame_size()) as u64;
        assert_eq!(segment.next_block_offset, offset3);
        let block = create_block(offset3);
        let block_sigs = create_block_sigs();
        segment.write_block(&block, &block_sigs).unwrap();
        {
            let block = segment.get_block(offset3).unwrap();
            assert_eq!(block.get_offset().unwrap(), offset3);
        }

        assert!(segment.get_block(10).is_err());
        assert!(segment.get_block(offset3 + 10).is_err());

        {
            let last_block = segment
                .get_block_from_next_offset(segment.next_block_offset)
                .unwrap();
            assert_eq!(last_block.get_offset().unwrap(), offset3);
        }
    }

    #[test]
    fn directory_segment_non_zero_offset_write() {
        let dir = tempdir::TempDir::new("test").unwrap();
        let config = Default::default();
        let segment_first_block_offset = 1234;

        {
            let first_block = create_block(segment_first_block_offset);
            let first_block_sigs = create_block_sigs();
            let mut segment =
                DirectorySegment::create(config, dir.path(), &first_block, &first_block_sigs)
                    .unwrap();
            let next_block_offset = segment.next_block_offset;
            assert_eq!(
                next_block_offset,
                segment_first_block_offset
                    + (first_block.frame_size() + first_block_sigs.frame_size()) as BlockOffset
            );
            append_blocks_to_segment(&mut segment, next_block_offset, 999);
        }

        {
            let segment = DirectorySegment::open_with_first_offset(
                config,
                dir.path(),
                segment_first_block_offset,
            )
            .unwrap();
            assert!(segment.get_block(0).is_err());
            assert!(segment.get_block(1234).is_ok());
            assert!(segment.get_block(segment.next_block_offset).is_err());
            assert!(segment
                .get_block_from_next_offset(segment.next_block_offset)
                .is_ok());

            let iter = msg::FramedMessageIterator::new(&segment.segment_file.mmap[0..]);
            assert_eq!(iter.count(), 2000); // blocks + sigs
        }
    }

    #[test]
    fn directory_segment_grow_and_truncate() {
        let mut config: DirectoryConfig = Default::default();
        config.segment_over_allocate_size = 100_000;

        let dir = tempdir::TempDir::new("test").unwrap();
        let mut next_offset = 0;

        let block = create_block(next_offset);
        let block_sigs = create_block_sigs();
        let mut segment =
            DirectorySegment::create(config, dir.path(), &block, &block_sigs).unwrap();
        next_offset += (block.frame_size() + block_sigs.frame_size()) as u64;

        let init_segment_size = segment.segment_file.current_size;
        append_blocks_to_segment(&mut segment, next_offset, 999);
        let end_segment_size = segment.segment_file.current_size;

        assert_eq!(init_segment_size, 100_000);
        assert!(end_segment_size >= 200_000);

        segment.truncate_extra().unwrap();

        let truncated_segment_size = segment.segment_file.current_size;
        assert!(truncated_segment_size < 200_000);

        let iter = msg::FramedMessageIterator::new(&segment.segment_file.mmap[0..]);
        assert_eq!(iter.count(), 2000); // blocks + sigs
    }

    #[test]
    fn segment_file_create() {
        let dir = tempdir::TempDir::new("test").unwrap();
        let segment_path = dir.path().join("segment_0.seg");

        let segment_file = SegmentFile::open(&segment_path, 1000).unwrap();
        assert_eq!(segment_file.current_size, 1000);
        drop(segment_file);

        let mut segment_file = SegmentFile::open(&segment_path, 10).unwrap();
        assert_eq!(segment_file.current_size, 1000);

        segment_file.set_len(2000).unwrap();
        assert_eq!(segment_file.current_size, 2000);
    }

    fn create_block(offset: u64) -> FramedOwnedTypedMessage<block::Owned> {
        let mut block_msg_builder = msg::MessageBuilder::<block::Owned>::new();
        {
            let mut block_builder = block_msg_builder.get_builder_typed();
            block_builder.set_hash("block_hash");
            block_builder.set_offset(offset);
        }
        block_msg_builder.as_owned_framed().unwrap()
    }

    fn create_block_sigs() -> FramedOwnedTypedMessage<block_signatures::Owned> {
        let mut block_msg_builder = msg::MessageBuilder::<block_signatures::Owned>::new();
        block_msg_builder.as_owned_framed().unwrap()
    }

    fn append_blocks_to_directory(
        directory_chain: &mut DirectoryStore,
        nb_blocks: usize,
        from_offset: BlockOffset,
    ) {
        let mut next_offset = from_offset;
        for _i in 0..nb_blocks {
            let block_msg = create_block(next_offset);
            let sig_msg = create_block_sigs();
            next_offset = directory_chain.write_block(&block_msg, &sig_msg).unwrap();
        }
    }

    fn validate_iterator(
        iter: StoredBlockIterator,
        expect_count: usize,
        expect_first_offset: BlockOffset,
        expect_last_offset: BlockOffset,
        reverse: bool,
    ) {
        let mut first_block_offset: Option<BlockOffset> = None;
        let mut last_block_offset: Option<BlockOffset> = None;
        let mut count = 0;

        for stored_block in iter {
            count += 1;

            let block_reader = stored_block.block.get_typed_reader().unwrap();
            let current_block_offset = block_reader.get_offset();
            if first_block_offset.is_none() {
                first_block_offset = Some(current_block_offset);
            }

            if let Some(last_block_offset) = last_block_offset {
                assert_eq!(
                    current_block_offset > last_block_offset,
                    !reverse,
                    "current offset > last offset"
                );
            }

            last_block_offset = Some(current_block_offset);
        }

        assert_eq!(count, expect_count);
        assert_eq!(first_block_offset.unwrap(), expect_first_offset);
        assert_eq!(last_block_offset.unwrap(), expect_last_offset);
    }

    fn append_blocks_to_segment(
        segment: &mut DirectorySegment,
        first_block_offset: BlockOffset,
        nb_blocks: usize,
    ) {
        let mut next_offset = first_block_offset;
        for _i in 0..nb_blocks {
            assert_eq!(next_offset, segment.next_block_offset);
            let block = create_block(next_offset);
            let block_sigs = create_block_sigs();
            segment.write_block(&block, &block_sigs).unwrap();
            next_offset += (block.frame_size() + block_sigs.frame_size()) as u64;
        }
    }
}
