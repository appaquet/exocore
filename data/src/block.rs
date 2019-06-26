use crate::operation::OperationId;
use exocore_common::cell::{Cell, FullCell};
use exocore_common::crypto::hash::{Digest, Multihash, MultihashDigest, Sha3_256};
use exocore_common::crypto::signature::Signature;
use exocore_common::data_chain_capnp::{
    block, block_operation_header, block_signature, block_signatures,
};
use exocore_common::framing;
use exocore_common::framing::{CapnpFrameBuilder, FrameBuilder, FrameReader};
use exocore_common::node::NodeId;
use exocore_common::serialization::framed::{
    OwnedTypedFrame, SignedFrame, TypedFrame, TypedSliceFrame,
};
use exocore_common::serialization::protos::data_chain_capnp::pending_operation;
use exocore_common::serialization::{capnp, framed};

pub type BlockOffset = u64;
pub type BlockDepth = u64;
pub type BlockOperationsSize = u32;
pub type BlockSignaturesSize = u16;

pub type BlockFrame<I> = framing::TypedCapnpFrame<framing::SizedFrame<I>, block::Owned>;

pub type SignaturesFrame<I> =
    framing::TypedCapnpFrame<framing::PaddedFrame<framing::SizedFrame<I>>, block_signatures::Owned>;

///
/// A trait representing a block stored or to be stored in the chain.
/// It can either be a referenced block (`BlockRef`) or a in-memory block (`BlockOwned`).
///
/// A block consists of 3 parts:
///  * Block header
///  * Operations' bytes (capnp serialized `pending_operation` frames)
///  * Block signatures
///
/// The block header and operations' data are the same on all nodes. Since a node writes a block
/// as soon as it has enough signatures, signatures can differ from one node to the other. Signatures
/// frame is pre-allocated, which means that not all signatures may fit. But in theory, it should always
/// contain enough space for all nodes to add their own signature.
///
pub trait Block {
    type SignaturesFrameData: framing::FrameReader<OwnedType = Vec<u8>>;

    type BlockType: TypedFrame<block::Owned> + SignedFrame;

    fn offset(&self) -> BlockOffset;
    fn block(&self) -> &Self::BlockType;
    fn operations_data(&self) -> &[u8];
    fn signatures(&self) -> &SignaturesFrame<Self::SignaturesFrameData>;

    #[inline]
    fn total_size(&self) -> usize {
        self.block().frame_size()
            + self.operations_data().len()
            + self.signatures().whole_data().len()
    }

    #[inline]
    fn next_offset(&self) -> BlockOffset {
        self.offset() + self.total_size() as BlockOffset
    }

    // TODO:Should return error
    #[inline]
    fn copy_data_into(&self, data: &mut [u8]) {
        let operations_data = self.operations_data();
        let operations_offset = self.block().frame_size();
        let signatures_offset = operations_offset + operations_data.len();

        self.block().copy_into(data);
        (&mut data[operations_offset..signatures_offset]).copy_from_slice(operations_data);
        self.signatures()
            .write_into(&mut data[signatures_offset..])
            .expect("Couldn't write signatures into given buffer");
    }

    fn as_data_vec(&self) -> Vec<u8> {
        vec![
            self.block().frame_data(),
            self.operations_data(),
            self.signatures().whole_data(),
        ]
        .concat()
    }

    fn to_owned(&self) -> BlockOwned {
        BlockOwned {
            offset: self.offset(),
            block: self.block().to_owned(),
            operations_data: self.operations_data().to_vec(),
            signatures: self.signatures().to_owned(),
        }
    }

    fn get_depth(&self) -> Result<BlockDepth, Error> {
        let reader = self.block().get_typed_reader()?;
        Ok(reader.get_depth())
    }

    fn get_proposed_operation_id(&self) -> Result<OperationId, Error> {
        let reader = self.block().get_typed_reader()?;
        Ok(reader.get_proposed_operation_id())
    }

    fn operations_iter(&self) -> Result<BlockOperationsIterator, Error> {
        let block_reader: block::Reader = self.block().get_typed_reader()?;
        let operations_header = block_reader
            .get_operations_header()?
            .iter()
            .map(|reader| BlockOperationHeader::from_reader(&reader))
            .collect::<Vec<_>>();

        Ok(BlockOperationsIterator {
            index: 0,
            operations_header,
            operations_data: self.operations_data(),
            last_error: None,
        })
    }

    fn get_operation(
        &self,
        operation_id: OperationId,
    ) -> Result<Option<TypedSliceFrame<pending_operation::Owned>>, Error> {
        // TODO: Implement binary search in operations, since they are sorted: https://github.com/appaquet/exocore/issues/43
        let operation = self.operations_iter()?.find(|operation| {
            if let Ok(operation_reader) = operation.get_typed_reader() {
                operation_reader.get_operation_id() == operation_id
            } else {
                false
            }
        });

        Ok(operation)
    }

    fn validate(&self) -> Result<(), Error> {
        // TODO: Should actually check signatures too

        let block_reader: block::Reader = self.block().get_typed_reader()?;

        let sig_size_header = block_reader.get_signatures_size() as usize;
        let sig_size_stored = self.signatures().whole_data().len();
        if sig_size_header != sig_size_stored {
            return Err(Error::Integrity(format!(
                "Signatures size don't match: sig_size_header={}, sig_size_stored={}",
                sig_size_header, sig_size_stored
            )));
        }

        let ops_size_header = block_reader.get_operations_size() as usize;
        let ops_size_stored = self.operations_data().len();
        if ops_size_header != ops_size_stored {
            return Err(Error::Integrity(format!(
                "Operations size don't match: ops_size_header={}, ops_size_stored={}",
                ops_size_header, ops_size_stored
            )));
        }

        if ops_size_header > 0 {
            let ops_hash_stored =
                BlockOperations::hash_operations(self.operations_iter()?)?.into_bytes();
            let ops_hash_header = block_reader.get_operations_hash()?;
            if ops_hash_stored != ops_hash_header {
                return Err(Error::Integrity(format!(
                    "Operations hash don't match: ops_hash_header={:?}, ops_hash_stored={:?}",
                    ops_hash_header, ops_hash_stored
                )));
            }
        }

        Ok(())
    }
}

///
/// Iterator over operations stored in a block.
///
pub struct BlockOperationsIterator<'a> {
    index: usize,
    operations_header: Vec<BlockOperationHeader>,
    operations_data: &'a [u8],
    last_error: Option<Error>,
}

impl<'a> Iterator for BlockOperationsIterator<'a> {
    type Item = TypedSliceFrame<'a, pending_operation::Owned>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.operations_header.len() {
            return None;
        }

        let header = &self.operations_header[self.index];
        self.index += 1;

        let offset_from = header.data_offset as usize;
        let offset_to = header.data_offset as usize + header.data_size as usize;

        let frame_res = TypedSliceFrame::new(&self.operations_data[offset_from..offset_to]);
        match frame_res {
            Ok(frame) => Some(frame),
            Err(err) => {
                self.last_error = Some(err.into());
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.index, Some(self.operations_data.len()))
    }
}

///
/// In-memory block.
///
pub struct BlockOwned {
    pub offset: BlockOffset,
    pub block: OwnedTypedFrame<block::Owned>,
    pub operations_data: Vec<u8>,
    pub signatures: SignaturesFrame<Vec<u8>>,
}

impl BlockOwned {
    pub fn new(
        offset: BlockOffset,
        block: OwnedTypedFrame<block::Owned>,
        operations_data: Vec<u8>,
        signatures: SignaturesFrame<Vec<u8>>,
    ) -> BlockOwned {
        BlockOwned {
            offset,
            block,
            operations_data,
            signatures,
        }
    }

    pub fn new_genesis(cell: &FullCell) -> Result<BlockOwned, Error> {
        let operations = BlockOperations::empty();
        let block = Self::new_with_prev_info(cell, 0, 0, 0, &[], 0, operations)?;

        // TODO: Add master signature after doing https://github.com/appaquet/exocore/issues/46

        Ok(block)
    }

    pub fn new_with_prev_block<B>(
        cell: &Cell,
        previous_block: &B,
        proposed_operation_id: u64,
        operations: BlockOperations,
    ) -> Result<BlockOwned, Error>
    where
        B: Block,
    {
        let previous_block_reader = previous_block.block().get_typed_reader()?;

        let previous_offset = previous_block.offset();
        let previous_hash = previous_block
            .block()
            .signature_data()
            .expect("Previous block didn't have a signature");

        let offset = previous_block.next_offset();
        let depth = previous_block_reader.get_depth();

        Self::new_with_prev_info(
            cell,
            offset,
            depth,
            previous_offset,
            previous_hash,
            proposed_operation_id,
            operations,
        )
    }

    pub fn new_with_prev_info(
        cell: &Cell,
        offset: BlockOffset,
        depth: BlockDepth,
        previous_offset: BlockOffset,
        previous_hash: &[u8],
        proposed_operation_id: u64,
        operations: BlockOperations,
    ) -> Result<BlockOwned, Error> {
        let local_node = cell.local_node();
        let operations_data_size = operations.data.len() as u32;

        // initialize block
        let mut block_frame_builder = framed::FrameBuilder::<block::Owned>::new();
        let mut block_builder: block::Builder = block_frame_builder.get_builder_typed();
        block_builder.set_offset(offset);
        block_builder.set_depth(depth + 1);
        block_builder.set_previous_offset(previous_offset);
        block_builder.set_previous_hash(previous_hash);
        block_builder.set_proposed_operation_id(proposed_operation_id);
        block_builder.set_proposed_node_id(local_node.id().to_str());
        block_builder.set_operations_size(operations_data_size);
        block_builder.set_operations_hash(&operations.multihash_bytes);

        let mut operations_builder = block_builder
            .reborrow()
            .init_operations_header(operations.headers.len() as u32);
        for (i, header_builder) in operations.headers.iter().enumerate() {
            let mut entry_builder = operations_builder.reborrow().get(i as u32);
            header_builder.copy_into_builder(&mut entry_builder);
        }

        // create an empty signature for each node as a placeholder to find the size required for signatures
        let signature_frame = BlockSignatures::empty_signatures_for_nodes(cell)
            .to_frame_for_new_block(operations_data_size)?;

        // set required signatures size in block
        block_builder.set_signatures_size(signature_frame.frame_size() as BlockSignaturesSize);
        let block_frame = block_frame_builder.as_owned_framed(local_node.frame_signer())?;

        Ok(BlockOwned {
            offset,
            block: block_frame,
            operations_data: operations.data,
            signatures: signature_frame,
        })
    }
}

impl Block for BlockOwned {
    type SignaturesFrameData = Vec<u8>;

    type BlockType = framed::OwnedTypedFrame<block::Owned>;

    fn offset(&self) -> u64 {
        self.offset
    }

    fn block(&self) -> &Self::BlockType {
        &self.block
    }

    fn operations_data(&self) -> &[u8] {
        &self.operations_data
    }

    fn signatures(&self) -> &SignaturesFrame<Self::SignaturesFrameData> {
        &self.signatures
    }
}

///
/// A referenced block
///
pub struct BlockRef<'a> {
    pub offset: BlockOffset,
    pub block: framed::TypedSliceFrame<'a, block::Owned>,
    pub operations_data: &'a [u8],
    pub signatures: SignaturesFrame<&'a [u8]>,
}

impl<'a> BlockRef<'a> {
    pub fn new(data: &[u8]) -> Result<BlockRef, Error> {
        let block = framed::TypedSliceFrame::new(data)?;
        let block_reader: block::Reader = block.get_typed_reader()?;

        let operations_offset = block.frame_size();
        let operations_size = block_reader.get_operations_size() as usize;
        let signatures_offset = operations_offset + operations_size;

        if signatures_offset >= data.len() {
            return Err(Error::OutOfBound(format!(
                "Signature offset {} is after data len {}",
                signatures_offset,
                data.len()
            )));
        }

        let operations_data = &data[operations_offset..operations_offset + operations_size];
        let signatures = BlockSignatures::read_frame_slice(&data[signatures_offset..])?;

        Ok(BlockRef {
            offset: block_reader.get_offset(),
            block,
            operations_data,
            signatures,
        })
    }

    pub fn new_from_next_offset(data: &[u8], next_offset: usize) -> Result<BlockRef, Error> {
        let signatures = BlockSignatures::read_frame_from_next_offset(data, next_offset)?;
        let signatures_reader: block_signatures::Reader = signatures.get_reader()?;
        let signatures_offset = next_offset - signatures.frame_size();

        let operations_size = signatures_reader.get_operations_size() as usize;
        if operations_size > signatures_offset {
            return Err(Error::OutOfBound(format!(
                "Tried to read block from next offset {}, but its operations size would exceed beginning of file (operations_size={} signatures_offset={})",
                next_offset, operations_size, signatures_offset,
            )));
        }

        let operations_offset = signatures_offset - operations_size;
        let operations_data = &data[operations_offset..operations_offset + operations_size];

        let block = framed::TypedSliceFrame::<block::Owned>::new_from_next_offset(
            &data[..],
            operations_offset,
        )?;
        let block_reader = block.get_typed_reader()?;

        Ok(BlockRef {
            offset: block_reader.get_offset(),
            operations_data,
            block,
            signatures,
        })
    }
}

impl<'a> Block for BlockRef<'a> {
    type SignaturesFrameData = &'a [u8];

    type BlockType = framed::TypedSliceFrame<'a, block::Owned>;

    fn offset(&self) -> u64 {
        self.offset
    }

    fn block(&self) -> &Self::BlockType {
        &self.block
    }

    fn operations_data(&self) -> &[u8] {
        &self.operations_data
    }

    fn signatures(&self) -> &SignaturesFrame<Self::SignaturesFrameData> {
        &self.signatures
    }
}

///
/// Block iterator over a slice of data.
///
pub struct ChainBlockIterator<'a> {
    current_offset: usize,
    data: &'a [u8],
    last_error: Option<Error>,
}

impl<'a> ChainBlockIterator<'a> {
    pub fn new(data: &'a [u8]) -> ChainBlockIterator<'a> {
        ChainBlockIterator {
            current_offset: 0,
            data,
            last_error: None,
        }
    }
}

impl<'a> Iterator for ChainBlockIterator<'a> {
    type Item = BlockRef<'a>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.current_offset >= self.data.len() {
            return None;
        }

        let block_res = BlockRef::new(&self.data[self.current_offset..]);
        match block_res {
            Ok(block) => {
                self.current_offset += block.total_size() as usize;
                Some(block)
            }
            Err(Error::Framing(framed::Error::EOF(_))) => None,
            Err(other) => {
                self.last_error = Some(other);
                None
            }
        }
    }
}

///
/// Wraps operations header stored in a block.
///
pub struct BlockOperations {
    multihash_bytes: Vec<u8>,
    headers: Vec<BlockOperationHeader>,
    data: Vec<u8>,
}

impl BlockOperations {
    pub fn empty() -> BlockOperations {
        BlockOperations {
            multihash_bytes: Vec::new(),
            headers: Vec::new(),
            data: Vec::new(),
        }
    }

    pub fn from_operations<I, F>(sorted_operations: I) -> Result<BlockOperations, Error>
    where
        I: Iterator<Item = F>,
        F: TypedFrame<pending_operation::Owned>,
    {
        let mut hasher = Sha3_256::new();
        let mut headers = Vec::new();
        let mut data = Vec::new();

        for operation in sorted_operations {
            let operation_reader = operation.get_typed_reader()?;
            let offset = data.len();
            let entry_data = operation.frame_data();
            hasher.input_signed_frame(&operation);
            data.extend_from_slice(entry_data);

            headers.push(BlockOperationHeader {
                operation_id: operation_reader.get_operation_id(),
                data_offset: offset as u32,
                data_size: (data.len() - offset) as u32,
            });
        }

        Ok(BlockOperations {
            multihash_bytes: hasher.into_multihash_bytes(),
            headers,
            data,
        })
    }

    pub fn hash_operations<I, F>(sorted_operations: I) -> Result<Multihash, Error>
    where
        I: Iterator<Item = F>,
        F: TypedFrame<pending_operation::Owned>,
    {
        let mut hasher = Sha3_256::new();
        for operation in sorted_operations {
            hasher.input_signed_frame(&operation);
        }
        Ok(hasher.into_multihash())
    }

    pub fn multihash_bytes(&self) -> &[u8] {
        &self.multihash_bytes
    }

    pub fn data(&self) -> &[u8] {
        &self.data
    }
}

///
/// Header of an operation stored within a block. It represents the position in the bytes of the block.
///
struct BlockOperationHeader {
    operation_id: u64,
    data_offset: u32,
    data_size: u32,
}

impl BlockOperationHeader {
    fn from_reader(reader: &block_operation_header::Reader) -> BlockOperationHeader {
        BlockOperationHeader {
            operation_id: reader.get_operation_id(),
            data_offset: reader.get_data_offset(),
            data_size: reader.get_data_size(),
        }
    }

    fn copy_into_builder(&self, builder: &mut block_operation_header::Builder) {
        builder.set_operation_id(self.operation_id);
        builder.set_data_size(self.data_size);
        builder.set_data_offset(self.data_offset);
    }
}

///
/// Represents signatures stored in a block. Since a node writes a block as soon as it has enough signatures, signatures can
/// differ from one node to the other. Signatures frame is pre-allocated, which means that not all signatures may fit. But in
/// theory, it should always contain enough space for all nodes to add their own signature.
///
pub struct BlockSignatures {
    signatures: Vec<BlockSignature>,
}

impl BlockSignatures {
    pub fn new_from_signatures(signatures: Vec<BlockSignature>) -> BlockSignatures {
        BlockSignatures { signatures }
    }

    ///
    /// Create signatures with pre-allocated space for the number of nodes we have in
    /// the cell
    ///
    pub fn empty_signatures_for_nodes(cell: &Cell) -> BlockSignatures {
        let nodes = cell.nodes();
        let signatures = nodes
            .iter()
            .all()
            .map(|node| BlockSignature {
                node_id: node.id().clone(),
                signature: Signature::empty(),
            })
            .collect();

        BlockSignatures { signatures }
    }

    fn to_frame_builder(&self) -> CapnpFrameBuilder<block_signatures::Owned> {
        let mut frame_builder = CapnpFrameBuilder::new();

        let signatures_builder: block_signatures::Builder = frame_builder.get_builder();
        let mut signatures_array = signatures_builder.init_signatures(self.signatures.len() as u32);
        for (i, signature) in self.signatures.iter().enumerate() {
            let mut signature_builder = signatures_array.reborrow().get(i as u32);
            signature.copy_into_builder(&mut signature_builder);
        }

        frame_builder
    }

    pub fn to_frame_for_new_block(
        &self,
        operations_size: BlockOperationsSize,
    ) -> Result<SignaturesFrame<Vec<u8>>, Error> {
        let mut signatures_frame_builder = self.to_frame_builder();
        let mut signatures_builder = signatures_frame_builder.get_builder();
        signatures_builder.set_operations_size(operations_size);

        let frame_builder = framing::SizedFrameBuilder::new(framing::PaddedFrameBuilder::new(
            signatures_frame_builder,
            0,
        ));
        let frame_data = frame_builder.as_bytes();
        Self::read_frame(frame_data)
    }

    pub fn to_frame_for_existing_block(
        &self,
        block_reader: &block::Reader,
    ) -> Result<SignaturesFrame<Vec<u8>>, Error> {
        let expected_signatures_size = usize::from(block_reader.get_signatures_size());

        // create capnp frame
        let mut signatures_frame_builder = self.to_frame_builder();
        let mut signatures_builder: block_signatures::Builder =
            signatures_frame_builder.get_builder();
        signatures_builder.set_operations_size(block_reader.get_operations_size());
        let signatures_frame_data = signatures_frame_builder.as_bytes();
        let signatures_frame_data_len = signatures_frame_data.len();

        // create the enclosure frame (sized & padded)
        let mut frame_builder = framing::SizedFrameBuilder::new(framing::PaddedFrameBuilder::new(
            signatures_frame_data,
            0,
        ));
        let frame_expected_size = frame_builder
            .expected_size()
            .expect("Frame should had been sized");

        // check if we need to add padding to match original signatures size
        if frame_expected_size < expected_signatures_size {
            let diff = expected_signatures_size - frame_expected_size;
            frame_builder
                .inner()
                .set_minimum_size(signatures_frame_data_len + diff);
        }

        // we build the frame and re-read it
        let frame_data = frame_builder.as_bytes();
        let signatures_frame = Self::read_frame(frame_data)?;

        // make sure that the signatures frame size is not higher than pre-allocated space in block
        if signatures_frame.frame_size() != expected_signatures_size {
            return Err(Error::Integrity(format!(
                "Block local signatures isn't the same size as expected (got={} expected={})",
                signatures_frame.frame_size(),
                block_reader.get_signatures_size()
            )));
        }

        Ok(signatures_frame)
    }

    pub fn read_frame(data: Vec<u8>) -> Result<SignaturesFrame<Vec<u8>>, Error> {
        let frame = framing::TypedCapnpFrame::new(framing::PaddedFrame::new(
            framing::SizedFrame::new(data)?,
        )?)?;

        Ok(frame)
    }

    pub fn read_frame_slice(data: &[u8]) -> Result<SignaturesFrame<&[u8]>, Error> {
        let frame = framing::TypedCapnpFrame::new(framing::PaddedFrame::new(
            framing::SizedFrame::new(data)?,
        )?)?;

        Ok(frame)
    }

    pub fn read_frame_from_next_offset(
        data: &[u8],
        next_offset: usize,
    ) -> Result<SignaturesFrame<&[u8]>, Error> {
        let frame = framing::TypedCapnpFrame::new(framing::PaddedFrame::new(
            framing::SizedFrame::new_from_next_offset(data, next_offset)?,
        )?)?;

        Ok(frame)
    }
}

///
/// Represents a signature of the block by one node, using its own key to sign the block's hash.
///
pub struct BlockSignature {
    pub node_id: NodeId,
    pub signature: Signature,
}

impl BlockSignature {
    pub fn new(node_id: NodeId, signature: Signature) -> BlockSignature {
        BlockSignature { node_id, signature }
    }

    pub fn copy_into_builder(&self, builder: &mut block_signature::Builder) {
        builder.set_node_id(self.node_id.to_str());
        builder.set_node_signature(self.signature.get_bytes());
    }
}

///
/// Block related errors
///
#[derive(Clone, Debug, Fail)]
pub enum Error {
    #[fail(display = "Block integrity error: {}", _0)]
    Integrity(String),
    #[fail(display = "An offset is out of the block data: {}", _0)]
    OutOfBound(String),
    #[fail(display = "Error in message serialization")]
    Framing(#[fail(cause)] framed::Error),
    #[fail(display = "IO error: {}", _0)]
    IO(String),
    #[fail(display = "Error in capnp serialization: kind={:?} msg={}", _0, _1)]
    Serialization(capnp::ErrorKind, String),
    #[fail(display = "Field is not in capnp schema: code={}", _0)]
    SerializationNotInSchema(u16),
    #[fail(display = "Other operation error: {}", _0)]
    Other(String),
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Framing(err)
    }
}

impl From<capnp::Error> for Error {
    fn from(err: capnp::Error) -> Self {
        Error::Serialization(err.kind, err.description)
    }
}

impl From<capnp::NotInSchema> for Error {
    fn from(err: capnp::NotInSchema) -> Self {
        Error::SerializationNotInSchema(err.0)
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IO(err.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exocore_common::node::LocalNode;

    #[test]
    fn should_allocate_signatures_space_for_nodes() -> Result<(), failure::Error> {
        let local_node = LocalNode::generate();
        let full_cell = FullCell::generate(local_node);
        let cell = full_cell.cell();
        let genesis_block = BlockOwned::new_genesis(&full_cell)?;

        let block_ops = BlockOperations::empty();
        let block1 = BlockOwned::new_with_prev_block(cell, &genesis_block, 0, block_ops)?;
        assert!(block1.signatures.frame_size() > 100);

        let node2 = LocalNode::generate();
        full_cell.nodes_mut().add(node2.node().clone());

        let block_ops = BlockOperations::empty();
        let block2 = BlockOwned::new_with_prev_block(cell, &genesis_block, 0, block_ops)?;
        assert!(block2.signatures.frame_size() > block1.signatures.frame_size());

        Ok(())
    }

    #[test]
    fn should_pad_signatures_from_block_signature_size() -> Result<(), failure::Error> {
        let local_node = LocalNode::generate();
        let full_cell = FullCell::generate(local_node);
        let cell = full_cell.cell();
        let genesis_block = BlockOwned::new_genesis(&full_cell)?;

        let block_ops = BlockOperations::empty();
        let block1 = BlockOwned::new_with_prev_block(cell, &genesis_block, 0, block_ops)?;
        let block1_reader: block::Reader = block1.block().get_typed_reader()?;

        // generate new signatures for existing block
        let block_signatures = BlockSignatures::new_from_signatures(Vec::new());
        let signatures_frame = block_signatures.to_frame_for_existing_block(&block1_reader)?;

        // new signatures frame should be the same size as the signatures specified in block
        assert_eq!(
            usize::from(block1_reader.get_signatures_size()),
            signatures_frame.frame_size()
        );

        Ok(())
    }
}
