use std::{
    ops::{Bound, Range, RangeBounds},
    sync::Arc,
};

use bytes::{Buf, Bytes};
use exocore_core::framing::FrameReader;
use exocore_protos::generated::data_chain_capnp::{block_header, block_signatures};

use crate::block::{
    read_header_frame, read_header_frame_from_next_offset, Block, BlockHeaderFrame, BlockOffset,
    BlockOwned, BlockSignatures, Error, SignaturesFrame,
};

pub trait Data: FrameReader<OwnedType = Bytes> + Clone {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8];

    fn view<R: RangeBounds<usize>>(&self, r: R) -> Self;

    fn len(&self) -> usize;

    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn to_bytes(&self) -> Bytes {
        Bytes::from(self.slice(..).to_vec())
    }
}

impl Data for Bytes {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8] {
        let r = translate_range(0, self.len(), r);
        &self.chunk()[r]
    }

    fn view<R: RangeBounds<usize>>(&self, r: R) -> Self {
        self.slice(r)
    }

    fn len(&self) -> usize {
        self.len()
    }
}

#[derive(Clone)]
pub struct MmapData {
    pub(crate) data: Arc<memmap2::Mmap>,
    pub(crate) start: usize,
    pub(crate) end: usize, // exclusive
}

impl MmapData {
    pub fn from_mmap(data: Arc<memmap2::Mmap>, len: usize) -> MmapData {
        MmapData {
            data,
            start: 0,
            end: len,
        }
    }
}

impl Data for MmapData {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8] {
        let r = translate_range(self.start, self.end, r);
        &self.data[r]
    }

    fn view<R: RangeBounds<usize>>(&self, r: R) -> MmapData {
        let r = translate_range(self.start, self.end, r);
        MmapData {
            data: self.data.clone(),
            start: r.start,
            end: r.end,
        }
    }

    fn len(&self) -> usize {
        self.end - self.start
    }
}

impl FrameReader for MmapData {
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

#[derive(Clone)]
pub enum SegmentData {
    Mmap(MmapData),
    Bytes(Bytes),
}

impl Data for SegmentData {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8] {
        match self {
            SegmentData::Mmap(m) => Data::slice(m, r),
            SegmentData::Bytes(m) => Data::slice(m, r),
        }
    }

    fn view<R: RangeBounds<usize>>(&self, r: R) -> SegmentData {
        match self {
            SegmentData::Mmap(m) => SegmentData::Mmap(Data::view(m, r)),
            SegmentData::Bytes(m) => SegmentData::Bytes(Data::view(m, r)),
        }
    }

    fn len(&self) -> usize {
        match self {
            SegmentData::Mmap(m) => Data::len(m),
            SegmentData::Bytes(m) => Data::len(m),
        }
    }
}

impl FrameReader for SegmentData {
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

#[derive(Clone)]
pub struct RefData<'s> {
    pub(crate) data: &'s [u8],
    pub(crate) start: usize,
    pub(crate) end: usize, // exclusive
}

impl<'s> RefData<'s> {
    pub fn new(data: &[u8]) -> RefData {
        RefData {
            data,
            start: 0,
            end: data.len(),
        }
    }
}

impl<'s> Data for RefData<'s> {
    fn slice<R: RangeBounds<usize>>(&self, r: R) -> &[u8] {
        let r = translate_range(self.start, self.end, r);
        &self.data[r]
    }

    fn view<R: RangeBounds<usize>>(&self, r: R) -> RefData<'s> {
        let r = translate_range(self.start, self.end, r);
        RefData {
            data: self.data,
            start: r.start,
            end: r.end,
        }
    }

    fn len(&self) -> usize {
        self.end - self.start
    }
}

impl<'s> FrameReader for RefData<'s> {
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

pub struct DataBlock<D: Data> {
    pub offset: BlockOffset,
    pub header: BlockHeaderFrame<D>,
    pub operations_data: D,
    pub signatures: SignaturesFrame<D>,
}

impl<D: Data> DataBlock<D> {
    pub fn new(data: D) -> Result<DataBlock<D>, Error> {
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

        Ok(DataBlock {
            offset: header_reader.get_offset(),
            header,
            operations_data,
            signatures,
        })
    }

    pub fn new_from_next_offset(data: D, next_offset: usize) -> Result<DataBlock<D>, Error> {
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

        Ok(DataBlock {
            offset: header_reader.get_offset(),
            operations_data,
            header,
            signatures,
        })
    }

    fn to_owned(&self) -> DataBlock<Bytes> {
        DataBlock {
            offset: self.offset,
            header: self.header.to_owned(),
            operations_data: self.operations_data.to_bytes(),
            signatures: self.signatures.to_owned(),
        }
    }
}

impl<D: Data> Block for DataBlock<D> {
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
