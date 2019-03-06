use std::ops::Range;

use exocore_common::data_chain_capnp::{block, block_signatures};
use exocore_common::serialization::framed;
use exocore_common::serialization::framed::TypedFrame;

pub type BlockOffset = u64;
pub type BlockDepth = u64;
pub type BlockSignaturesSize = u16;

pub mod directory;

pub trait Store: Send + Sync + 'static {
    fn write_block<B, S>(&mut self, block: &B, block_signatures: &S) -> Result<BlockOffset, Error>
    where
        B: framed::TypedFrame<block::Owned>,
        S: framed::TypedFrame<block_signatures::Owned>;

    fn available_segments(&self) -> Vec<Range<BlockOffset>>;

    fn block_iter(&self, from_offset: BlockOffset) -> Result<StoredBlockIterator, Error>;

    fn block_iter_reverse(
        &self,
        from_next_offset: BlockOffset,
    ) -> Result<StoredBlockIterator, Error>;

    fn get_block(&self, offset: BlockOffset) -> Result<StoredBlock, Error>;

    fn get_block_from_next_offset(&self, next_offset: BlockOffset) -> Result<StoredBlock, Error>;

    fn get_last_block(&self) -> Result<Option<StoredBlock>, Error>;

    fn truncate_from_offset(&mut self, offset: BlockOffset) -> Result<(), Error>;
}

#[derive(Debug, Clone, PartialEq, Fail)]
pub enum Error {
    #[fail(display = "The store is in an unexpected state: {}", _0)]
    UnexpectedState(String),
    #[fail(display = "Error from the framing serialization: {:?}", _0)]
    Framing(#[fail(cause)] framed::Error),
    #[fail(display = "The store has an integrity problem: {}", _0)]
    Integrity(String),
    #[fail(display = "A segment has reached its full capacity")]
    SegmentFull,
    #[fail(display = "An offset is out of the chain data: {}", _0)]
    OutOfBound(String),
    #[fail(display = "IO error of kind {:?}: {}", _0, _1)]
    IO(std::io::ErrorKind, String),
}

pub struct StoredBlock<'a> {
    pub offset: BlockOffset,
    pub block: framed::TypedSliceFrame<'a, block::Owned>,
    pub signatures: framed::TypedSliceFrame<'a, block_signatures::Owned>,
}

impl<'a> StoredBlock<'a> {
    #[inline]
    pub fn total_size(&self) -> usize {
        self.block.frame_size() + self.signatures.frame_size()
    }

    #[inline]
    pub fn next_offset(&self) -> BlockOffset {
        self.offset + self.total_size() as BlockOffset
    }
}

type StoredBlockIterator<'pers> = Box<dyn Iterator<Item = StoredBlock<'pers>> + 'pers>;

pub enum EntryType {
    Data,
    Truncate,
    Duplicate,
}

pub struct EntryData {
    data: Vec<u8>,
}
