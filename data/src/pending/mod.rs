use std::ops::RangeBounds;
use std::sync::Arc;
use std::vec::Vec;

use exocore_common::data_chain_capnp::{operation_entry_new, pending_operation};
use exocore_common::serialization::framed::FrameBuilder;
use exocore_common::serialization::protos::{GroupID, OperationID};
use exocore_common::serialization::{capnp, framed};

pub mod memory;

///
///
///
pub trait Store: Send + Sync + 'static {
    fn put_operation(
        &mut self,
        operation: framed::OwnedTypedFrame<pending_operation::Owned>,
    ) -> Result<(), Error>;

    fn get_group_operations(
        &self,
        group_id: GroupID,
    ) -> Result<Option<StoredOperationsGroup>, Error>;

    fn operations_iter<R>(&self, range: R) -> Result<TimelineIterator, Error>
    where
        R: RangeBounds<OperationID>;
}

pub type TimelineIterator<'store> = Box<dyn Iterator<Item = StoredOperation> + 'store>;

///
///
///
#[derive(Clone)]
pub struct StoredOperation {
    pub group_id: GroupID,
    pub operation_id: OperationID,
    pub operation_type: OperationType,
    pub frame: Arc<framed::OwnedTypedFrame<pending_operation::Owned>>,
}

pub struct StoredOperationsGroup {
    pub group_id: GroupID,
    pub operations: Vec<StoredOperation>,
}

///
///
///
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum OperationType {
    EntryNew,
    BlockPropose,
    BlockSign,
    BlockRefuse,
    PendingIgnore,
}

///
///
///
pub struct PendingOperation {}

impl PendingOperation {
    pub fn new_entry(data: &[u8]) -> FrameBuilder<pending_operation::Owned> {
        let mut frame_builder = FrameBuilder::new();
        let operation_builder: pending_operation::Builder = frame_builder.get_builder_typed();

        let inner_operation_builder: pending_operation::operation::Builder =
            operation_builder.init_operation();
        let mut new_entry_builder: operation_entry_new::Builder =
            inner_operation_builder.init_entry_new();
        new_entry_builder.set_data(data);

        frame_builder
    }
}

///
///
///
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Error in message serialization")]
    Serialization(#[fail(cause)] framed::Error),
    #[fail(display = "Field is not in capnp schema: code={}", _0)]
    SerializationNotInSchema(u16),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        false
    }
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Serialization(err)
    }
}

impl From<capnp::NotInSchema> for Error {
    fn from(err: capnp::NotInSchema) -> Self {
        Error::SerializationNotInSchema(err.0)
    }
}

#[cfg(test)]
pub mod tests {
    use exocore_common::serialization::framed::{FrameBuilder, MultihashFrameSigner};

    use super::*;

    pub fn create_new_entry_op(
        operation_id: OperationID,
        group_id: GroupID,
    ) -> framed::OwnedTypedFrame<pending_operation::Owned> {
        let mut msg_builder = FrameBuilder::<pending_operation::Owned>::new();

        {
            let mut op_builder: pending_operation::Builder = msg_builder.get_builder_typed();
            op_builder.set_group_id(group_id);
            op_builder.set_operation_id(operation_id);

            let inner_op_builder = op_builder.init_operation();
            let mut new_entry_builder = inner_op_builder.init_entry_new();

            new_entry_builder.set_data(b"bob");
        }

        let frame_signer = MultihashFrameSigner::new_sha3256();
        msg_builder.as_owned_framed(frame_signer).unwrap()
    }
}
