use std::ops::Range;

use exocore_common::data_chain_capnp::{
    block, block_entry_header, block_signature, block_signatures,
};
use exocore_common::node::{Node, NodeID, Nodes};
use exocore_common::security::hash::{Sha3Hasher, StreamHasher};
use exocore_common::security::signature::Signature;
use exocore_common::serialization::framed::{
    FrameBuilder, OwnedTypedFrame, SignedFrame, TypedFrame, TypedSliceFrame,
};
use exocore_common::serialization::protos::data_chain_capnp::pending_operation;
use exocore_common::serialization::{capnp, framed};

pub type BlockOffset = u64;
pub type BlockDepth = u64;
pub type BlockSignaturesSize = u16;

pub mod directory;

pub trait Store: Send + Sync + 'static {
    // TODO: Validate that the block signature has same size as the actual signatures
    fn write_block<B: Block>(&mut self, block: &B) -> Result<BlockOffset, Error>;

    fn available_segments(&self) -> Vec<Range<BlockOffset>>;

    fn block_iter(&self, from_offset: BlockOffset) -> Result<StoredBlockIterator, Error>;

    fn block_iter_reverse(
        &self,
        from_next_offset: BlockOffset,
    ) -> Result<StoredBlockIterator, Error>;

    fn get_block(&self, offset: BlockOffset) -> Result<BlockRef, Error>;

    fn get_block_from_next_offset(&self, next_offset: BlockOffset) -> Result<BlockRef, Error>;

    fn get_last_block(&self) -> Result<Option<BlockRef>, Error>;

    fn truncate_from_offset(&mut self, offset: BlockOffset) -> Result<(), Error>;
}

type StoredBlockIterator<'pers> = Box<dyn Iterator<Item = BlockRef<'pers>> + 'pers>;

///
///
///
pub trait Block {
    type BlockType: TypedFrame<block::Owned> + SignedFrame;
    type SignaturesType: TypedFrame<block_signatures::Owned> + SignedFrame;

    fn offset(&self) -> BlockOffset;
    fn block(&self) -> &Self::BlockType;
    fn entries_data(&self) -> &[u8];
    fn signatures(&self) -> &Self::SignaturesType;

    #[inline]
    fn total_size(&self) -> usize {
        self.block().frame_size() + self.entries_data().len() + self.signatures().frame_size()
    }

    #[inline]
    fn next_offset(&self) -> BlockOffset {
        self.offset() + self.total_size() as BlockOffset
    }

    #[inline]
    fn copy_into(&self, data: &mut [u8]) {
        let entries_data = self.entries_data();
        let entries_offset = self.block().frame_size();
        let signatures_offset = entries_offset + entries_data.len();

        self.block().copy_into(data);
        (&mut data[entries_offset..signatures_offset]).copy_from_slice(entries_data);
        self.signatures().copy_into(&mut data[signatures_offset..]);
    }

    fn entries_iter(&self) -> Result<BlockEntriesIterator, Error> {
        let block_reader: block::Reader = self.block().get_typed_reader()?;
        let entries_header = block_reader
            .get_entries_header()?
            .iter()
            .map(|reader| EntryHeader::from_reader(&reader))
            .collect::<Vec<_>>();

        Ok(BlockEntriesIterator {
            index: 0,
            entries_header,
            entries_data: self.entries_data(),
            last_error: None,
        })
    }

    fn validate(&self) -> Result<(), Error> {
        // TODO: check signature size in block and signatures
        Ok(())
    }
}

///
///
///
pub struct BlockEntriesIterator<'a> {
    index: usize,
    entries_header: Vec<EntryHeader>,
    entries_data: &'a [u8],
    last_error: Option<Error>,
}

impl<'a> Iterator for BlockEntriesIterator<'a> {
    type Item = TypedSliceFrame<'a, pending_operation::Owned>;

    fn next(&mut self) -> Option<Self::Item> {
        if self.index >= self.entries_header.len() {
            return None;
        }

        let header = &self.entries_header[self.index];
        self.index += 1;

        let offset_from = header.data_offset as usize;
        let offset_to = header.data_offset as usize + header.data_size as usize;

        let frame_res = TypedSliceFrame::new(&self.entries_data[offset_from..offset_to]);
        match frame_res {
            Ok(frame) => Some(frame),
            Err(err) => {
                self.last_error = Some(err.into());
                None
            }
        }
    }

    fn size_hint(&self) -> (usize, Option<usize>) {
        (self.index, Some(self.entries_data.len()))
    }
}

///
///
///
pub struct BlockOwned {
    pub offset: BlockOffset,
    pub block: OwnedTypedFrame<block::Owned>,
    pub entries_data: Vec<u8>,
    pub signatures: OwnedTypedFrame<block_signatures::Owned>,
}

impl BlockOwned {
    pub fn new(
        offset: BlockOffset,
        block: OwnedTypedFrame<block::Owned>,
        entries_data: Vec<u8>,
        signatures: OwnedTypedFrame<block_signatures::Owned>,
    ) -> BlockOwned {
        BlockOwned {
            offset,
            block,
            entries_data,
            signatures,
        }
    }

    pub fn new_genesis(nodes: &Nodes, node: &Node) -> Result<BlockOwned, Error> {
        let operations: Vec<OwnedTypedFrame<pending_operation::Owned>> = Vec::new();
        let block =
            Self::new_from_operations(nodes, node, 0, 0, 0, &[], 0, operations.into_iter())?;

        // TODO: Add master signature

        Ok(block)
    }

    pub fn new_from_previous_and_operations<B, I, O>(
        nodes: &Nodes,
        node: &Node,
        offset: BlockOffset,
        depth: BlockDepth,
        previous_block: &B,
        proposed_operation_id: u64,
        operations: I,
    ) -> Result<BlockOwned, Error>
    where
        B: Block,
        I: Iterator<Item = O>,
        O: TypedFrame<pending_operation::Owned>,
    {
        let previous_offset = previous_block.offset();
        let previous_hash = previous_block
            .block()
            .signature_data()
            .expect("Previous block didn't have a signature");

        Self::new_from_operations(
            nodes,
            node,
            offset,
            depth,
            previous_offset,
            previous_hash,
            proposed_operation_id,
            operations,
        )
    }

    pub fn new_from_operations<I, O>(
        nodes: &Nodes,
        node: &Node,
        offset: BlockOffset,
        depth: BlockDepth,
        previous_offset: BlockOffset,
        previous_hash: &[u8],
        proposed_operation_id: u64,
        operations: I,
    ) -> Result<BlockOwned, Error>
    where
        I: Iterator<Item = O>,
        O: TypedFrame<pending_operation::Owned>,
    {
        let mut hasher = Sha3Hasher::new_256();
        let mut entries_header = Vec::new();
        let mut entries_data = Vec::new();
        for operation in operations {
            let operation_reader = operation.get_typed_reader()?;
            let offset = entries_data.len();
            let data = operation.frame_data();
            hasher.consume(data);
            entries_data.extend_from_slice(data);

            entries_header.push(EntryHeader {
                operation_id: operation_reader.get_operation_id(),
                data_offset: offset as u32,
                data_size: data.len() as u32,
            });
        }

        // initialize block
        let mut block_frame_builder = FrameBuilder::<block::Owned>::new();
        let mut block_builder: block::Builder = block_frame_builder.get_builder_typed();
        block_builder.set_offset(offset);
        block_builder.set_depth(depth);
        block_builder.set_previous_offset(previous_offset);
        block_builder.set_previous_hash(previous_hash);
        block_builder.set_proposed_operation_id(proposed_operation_id);
        block_builder.set_proposed_node_id(&node.id);
        block_builder.set_entries_size(entries_data.len() as u32);
        block_builder.set_entries_hash(&hasher.into_multihash_bytes());

        // TODO: Operations should be sorted
        let mut entries_builder = block_builder
            .reborrow()
            .init_entries_header(entries_header.len() as u32);
        for (i, header_builder) in entries_header.iter().enumerate() {
            let mut entry_builder = entries_builder.reborrow().get(i as u32);
            header_builder.copy_into_builder(&mut entry_builder);
        }

        // create empty signature with all nodes placeholders
        let mut signature_frame_builder =
            BlockSignatures::empty_signatures_for_nodes(nodes).to_frame_builder();
        let mut signature_builder = signature_frame_builder.get_builder_typed();
        signature_builder.set_entries_size(entries_data.len() as u32);
        let signature_frame = signature_frame_builder.as_owned_framed(node.frame_signer())?;

        // set signature size in block
        block_builder.set_signatures_size(signature_frame.frame_size() as u16);
        let block_frame = block_frame_builder.as_owned_framed(node.frame_signer())?;

        Ok(BlockOwned {
            offset,
            block: block_frame,
            entries_data,
            signatures: signature_frame,
        })
    }
}

impl Block for BlockOwned {
    type BlockType = framed::OwnedTypedFrame<block::Owned>;
    type SignaturesType = framed::OwnedTypedFrame<block_signatures::Owned>;

    #[inline]
    fn offset(&self) -> u64 {
        self.offset
    }

    #[inline]
    fn block(&self) -> &Self::BlockType {
        &self.block
    }

    #[inline]
    fn entries_data(&self) -> &[u8] {
        &self.entries_data
    }

    #[inline]
    fn signatures(&self) -> &Self::SignaturesType {
        &self.signatures
    }
}

///
///
///
pub struct BlockRef<'a> {
    pub offset: BlockOffset,
    pub block: framed::TypedSliceFrame<'a, block::Owned>,
    pub entries_data: &'a [u8],
    pub signatures: framed::TypedSliceFrame<'a, block_signatures::Owned>,
}

impl<'a> BlockRef<'a> {
    pub fn new(data: &[u8]) -> Result<BlockRef, Error> {
        let block = framed::TypedSliceFrame::new(data)?;
        let block_reader: block::Reader = block.get_typed_reader()?;

        let entries_offset = block.frame_size();
        let entries_size = block_reader.get_entries_size() as usize;
        let signatures_offset = entries_offset + entries_size;

        if signatures_offset >= data.len() {
            return Err(Error::OutOfBound(format!("Signature offset {} is after data len {}", signatures_offset, data.len())));
        }

        let entries_data = &data[entries_offset..entries_offset + entries_size];
        let signatures = framed::TypedSliceFrame::new(&data[signatures_offset..])?;

        Ok(BlockRef {
            offset: block_reader.get_offset(),
            block,
            entries_data,
            signatures,
        })
    }
}

impl<'a> Block for BlockRef<'a> {
    type BlockType = framed::TypedSliceFrame<'a, block::Owned>;
    type SignaturesType = framed::TypedSliceFrame<'a, block_signatures::Owned>;

    #[inline]
    fn offset(&self) -> u64 {
        self.offset
    }

    #[inline]
    fn block(&self) -> &Self::BlockType {
        &self.block
    }

    #[inline]
    fn entries_data(&self) -> &[u8] {
        &self.entries_data
    }

    #[inline]
    fn signatures(&self) -> &Self::SignaturesType {
        &self.signatures
    }
}

///
///
///
struct EntryHeader {
    operation_id: u64,
    data_offset: u32,
    data_size: u32,
}

impl EntryHeader {
    fn from_reader(reader: &block_entry_header::Reader) -> EntryHeader {
        EntryHeader {
            operation_id: reader.get_operation_id(),
            data_offset: reader.get_data_offset(),
            data_size: reader.get_data_size(),
        }
    }

    fn copy_into_builder(&self, builder: &mut block_entry_header::Builder) {
        builder.set_operation_id(self.operation_id);
        builder.set_data_size(self.data_size);
        builder.set_data_offset(self.data_offset);
    }
}

///
///
///
pub struct BlockSignatures {
    signatures: Vec<BlockSignature>,
}

impl BlockSignatures {
    pub fn empty_signatures_for_nodes(nodes: &Nodes) -> BlockSignatures {
        let signatures = nodes
            .nodes()
            .map(|node| BlockSignature {
                node_id: node.id().clone(),
                signature: Signature::empty(),
            })
            .collect();

        BlockSignatures { signatures }
    }

    pub fn to_frame_builder(&self) -> FrameBuilder<block_signatures::Owned> {
        let mut frame_builder = FrameBuilder::new();

        let signatures_builder: block_signatures::Builder = frame_builder.get_builder_typed();
        let mut signatures_array = signatures_builder.init_signatures(self.signatures.len() as u32);
        for (i, signature) in self.signatures.iter().enumerate() {
            let mut signature_builder = signatures_array.reborrow().get(i as u32);
            signature.copy_into_builder(&mut signature_builder);
        }

        frame_builder
    }
}

pub struct BlockSignature {
    pub node_id: NodeID,
    pub signature: Signature,
}

impl BlockSignature {
    pub fn new(node_id: NodeID, signature: Signature) -> BlockSignature {
        BlockSignature { node_id, signature }
    }

    pub fn copy_into_builder(&self, builder: &mut block_signature::Builder) {
        builder.set_node_id(&self.node_id);
        builder.set_node_signature(self.signature.get_bytes());
    }
}

///
///
///
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
    #[fail(display = "Field is not in capnp schema: code={}", _0)]
    SerializationNotInSchema(u16),
    #[fail(display = "Error in capnp serialization: kind={:?} msg={}", _0, _1)]
    Serialization(capnp::ErrorKind, String),
    #[fail(display = "An error occurred: {}", _0)]
    Other(String),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        match self {
            Error::UnexpectedState(_) | Error::Integrity(_) | Error::IO(_, _) => true,
            _ => false,
        }
    }
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Framing(err)
    }
}

impl From<capnp::NotInSchema> for Error {
    fn from(err: capnp::NotInSchema) -> Self {
        Error::SerializationNotInSchema(err.0)
    }
}

impl From<capnp::Error> for Error {
    fn from(err: capnp::Error) -> Self {
        Error::Serialization(err.kind, err.description)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::pending::PendingOperation;

    #[test]
    fn test_block_create_and_read() -> Result<(), failure::Error> {
        let mut nodes = Nodes::new();
        let node1 = Node::new("node1".to_string());
        nodes.add(node1.clone());

        let first_block = BlockOwned::new_genesis(&nodes, &node1)?;

        let mut operations: Vec<OwnedTypedFrame<pending_operation::Owned>> = Vec::new();
        let operation =
            PendingOperation::new_entry(b"some_data").as_owned_framed(node1.frame_signer())?;
        operations.push(operation);
        let second_block = BlockOwned::new_from_previous_and_operations(
            &nodes,
            &node1,
            first_block.next_offset(),
            1,
            &first_block,
            0,
            operations.into_iter(),
        )?;

        let mut data = [0u8; 5000];
        second_block.copy_into(&mut data);

        let read_second_block = BlockRef::new(&data[0..second_block.total_size()])?;
        assert_eq!(
            second_block.block.frame_data(),
            read_second_block.block.frame_data()
        );
        assert_eq!(second_block.entries_data, read_second_block.entries_data);
        assert_eq!(
            second_block.signatures.frame_data(),
            read_second_block.signatures.frame_data()
        );

        let block_reader: block::Reader = second_block.block.get_typed_reader()?;
        assert_eq!(block_reader.get_offset(), first_block.next_offset());
        assert_eq!(
            block_reader.get_signatures_size(),
            second_block.signatures.frame_size() as u16
        );
        assert_eq!(
            block_reader.get_entries_size(),
            second_block.entries_data.len() as u32
        );

        let signatures_reader: block_signatures::Reader =
            second_block.signatures.get_typed_reader()?;
        assert_eq!(
            signatures_reader.get_entries_size(),
            second_block.entries_data.len() as u32
        );

        let signatures = signatures_reader.get_signatures()?;
        assert_eq!(signatures.len(), 1);

        Ok(())
    }

    #[test]
    fn test_block_operations() -> Result<(), failure::Error> {
        let mut nodes = Nodes::new();
        let node1 = Node::new("node1".to_string());
        nodes.add(node1.clone());
        let genesis = BlockOwned::new_genesis(&nodes, &node1)?;

        // 0 operations
        let operations: Vec<OwnedTypedFrame<pending_operation::Owned>> = Vec::new();
        let block = BlockOwned::new_from_previous_and_operations(
            &nodes,
            &node1,
            genesis.next_offset(),
            1,
            &genesis,
            0,
            operations.into_iter(),
        )?;
        assert_eq!(block.entries_iter()?.count(), 0);

        // 5 operations
        let mut operations: Vec<OwnedTypedFrame<pending_operation::Owned>> = Vec::new();
        for _i in 0..5 {
            let operation =
                PendingOperation::new_entry(b"op1").as_owned_framed(node1.frame_signer())?;
            operations.push(operation);
        }
        let block = BlockOwned::new_from_previous_and_operations(
            &nodes,
            &node1,
            genesis.next_offset(),
            1,
            &genesis,
            0,
            operations.into_iter(),
        )?;
        assert_eq!(block.entries_iter()?.count(), 5);

        Ok(())
    }
}
