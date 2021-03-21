use std::{
    ops::{Bound, Range, RangeBounds},
    sync::Arc,
};

use bytes::Bytes;
use exocore_core::framing::FrameReader;
use exocore_protos::generated::data_chain_capnp::{block_header, block_signatures};

use crate::block::{
    read_header_frame, read_header_frame_from_next_offset, Block, BlockHeaderFrame, BlockOffset,
    BlockSignatures, Error, SignaturesFrame,
};

pub trait Data: FrameReader<OwnedType = Bytes> + Clone {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8];
    fn view<R: RangeBounds<usize>>(&self, r: R) -> Self;
    fn len(&self) -> usize;
    fn is_empty(&self) -> bool {
        self.len() == 0
    }
}

#[derive(Clone)]
pub enum StaticDataContainer {
    Mmap(Arc<memmap2::Mmap>),
    Bytes(Bytes),
}

#[derive(Clone)]
pub struct StaticData {
    pub(crate) data: StaticDataContainer,
    pub(crate) start: usize,
    pub(crate) end: usize, // exclusive
}

impl Data for StaticData {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8] {
        let r = translate_range(self.start, self.end, r);
        match &self.data {
            StaticDataContainer::Mmap(mmap) => &mmap[r],
            StaticDataContainer::Bytes(bytes) => &bytes[r],
        }
    }

    fn view<R: RangeBounds<usize>>(&self, r: R) -> StaticData {
        let r = translate_range(self.start, self.end, r);
        StaticData {
            data: self.data.clone(),
            start: r.start,
            end: r.end,
        }
    }

    fn len(&self) -> usize {
        self.end - self.start
    }
}

impl FrameReader for StaticData {
    type OwnedType = Bytes;

    fn exposed_data(&self) -> &[u8] {
        self.slice(..)
    }

    fn whole_data(&self) -> &[u8] {
        self.slice(..)
    }

    fn to_owned_frame(&self) -> Self::OwnedType {
        panic!("Cannot do to_owned_frame since it could be a whole mmap")
    }
}

fn translate_range<R: RangeBounds<usize>>(start: usize, end: usize, range: R) -> Range<usize> {
    let new_start = match range.start_bound() {
        Bound::Included(s) => start + *s,
        Bound::Excluded(s) => start + *s + 1,
        Bound::Unbounded => start,
    };
    let new_end = match range.end_bound() {
        Bound::Included(s) => (start + *s + 1).min(end),
        Bound::Excluded(s) => (start + *s).min(end),
        Bound::Unbounded => end,
    };

    Range {
        start: new_start,
        end: new_end,
    }
}

pub struct SegmentBlock<D: Data> {
    pub offset: BlockOffset,
    pub header: BlockHeaderFrame<D>,
    pub operations_data: D,
    pub signatures: SignaturesFrame<D>,
}

impl<D: Data> SegmentBlock<D> {
    pub fn new(data: D) -> Result<SegmentBlock<D>, Error> {
        let header = read_header_frame(data.clone())?;
        let header_reader: block_header::Reader = header.get_reader()?;

        let operations_offset = header.whole_data_size();
        let operations_size = header_reader.get_operations_size() as usize;
        let signatures_offset = operations_offset + operations_size;
        let signatures_size = header_reader.get_signatures_size() as usize;

        if signatures_offset >= data.len() {
            return Err(Error::OutOfBound(format!(
                "Signature offset {} is after data len {}",
                signatures_offset,
                data.len()
            )));
        }

        let signatures_data = data.view(signatures_offset..signatures_offset + signatures_size);
        let signatures = BlockSignatures::read_frame(signatures_data)?;

        let operations_data = data.view(operations_offset..signatures_offset);

        Ok(SegmentBlock {
            offset: header_reader.get_offset(),
            header,
            operations_data,
            signatures,
        })
    }

    pub fn new_from_next_offset(
        data: D,
        next_offset: usize,
    ) -> Result<SegmentBlock<D>, Error> {
        let signatures = BlockSignatures::read_frame_from_next_offset(data.clone(), next_offset)?;
        let signatures_reader: block_signatures::Reader = signatures.get_reader()?;
        let signatures_offset = next_offset - signatures.whole_data_size();

        let operations_size = signatures_reader.get_operations_size() as usize;
        if operations_size > signatures_offset {
            return Err(Error::OutOfBound(format!(
                "Tried to read block from next offset {}, but its operations size would exceed beginning of file (operations_size={} signatures_offset={})",
                next_offset, operations_size, signatures_offset,
            )));
        }

        let operations_offset = signatures_offset - operations_size;
        let operations_data = data.view(operations_offset..signatures_offset);

        let header = read_header_frame_from_next_offset(data, operations_offset)?;
        let header_reader: block_header::Reader = header.get_reader()?;

        Ok(SegmentBlock {
            offset: header_reader.get_offset(),
            operations_data,
            header,
            signatures,
        })
    }
}

impl<D: Data> Block for SegmentBlock<D> {
    type UnderlyingFrame = D;

    fn offset(&self) -> u64 {
        self.offset
    }

    fn header(&self) -> &BlockHeaderFrame<Self::UnderlyingFrame> {
        &self.header
    }

    fn operations_data(&self) -> &[u8] {
        self.operations_data.slice(..)
    }

    fn signatures(&self) -> &SignaturesFrame<Self::UnderlyingFrame> {
        &self.signatures
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_translate_range() {
        assert_eq!(translate_range(0, 99, ..), Range { start: 0, end: 99 });
        assert_eq!(translate_range(2, 99, ..), Range { start: 2, end: 99 });
        assert_eq!(translate_range(2, 99, ..120), Range { start: 2, end: 99 });
        assert_eq!(translate_range(10, 99, 0..9), Range { start: 10, end: 19 });
        assert_eq!(translate_range(10, 99, 0..=9), Range { start: 10, end: 20 });
        assert_eq!(translate_range(10, 99, ..9), Range { start: 10, end: 19 });
        assert_eq!(translate_range(10, 99, ..10), Range { start: 10, end: 20 });
        assert_eq!(translate_range(10, 99, 80..), Range { start: 90, end: 99 });
    }
}
