#![allow(dead_code)]

use exocore_common::security::hash::{Sha3Hasher, StreamHasher};
use exocore_common::serialization::framed::{
    FrameBuilder, OwnedTypedFrame, SignedFrame, TypedFrame,
};
use exocore_common::serialization::protos::data_transport_capnp::{
    pending_sync_range, pending_sync_request, pending_sync_response,
};
use exocore_common::serialization::protos::OperationID;
use exocore_common::serialization::{capnp, framed};

use crate::pending;
use crate::pending::{Store, StoredOperation};

pub struct Synchronizer<PS: Store> {
    phantom: std::marker::PhantomData<PS>,
}

impl<PS: Store> Synchronizer<PS> {
    pub fn new() -> Synchronizer<PS> {
        Synchronizer {
            phantom: std::marker::PhantomData,
        }
    }

    pub fn create_sync_range_request(
        &self,
        store: &PS,
    ) -> Result<FrameBuilder<pending_sync_request::Owned>, Error> {
        let mut operations_iter = store.operations_iter(..)?;

        unimplemented!()
    }

    pub fn handle_incoming_sync_request(
        &mut self,
        store: &mut PS,
    ) -> Result<Option<FrameBuilder<pending_sync_response::Owned>>, Error> {
        //TODO: Check if we need to answer
        unimplemented!()
    }
}

///
///
///
struct SyncRanges {
    ranges: Vec<SyncRange>,
    max_range_operations: usize,
}

impl SyncRanges {
    fn new(max_range_operations: usize) -> SyncRanges {
        SyncRanges {
            ranges: Vec::new(),
            max_range_operations,
        }
    }

    fn push_operation(&mut self, operation: StoredOperation) {
        let last_range_size = self.ranges.last().map(|r| r.operations.len()).unwrap_or(0);
        if self.ranges.is_empty() || last_range_size > self.max_range_operations {
            self.push_new_range();
        }

        if let Some(last_range) = self.ranges.last_mut() {
            last_range.push_operation(operation);
        }
    }

    fn push_new_range(&mut self) {
        let last_range_to = self.ranges.last().map(|r| r.to_operation).unwrap_or(0);
        self.ranges.push(SyncRange::new(last_range_to, 0));
    }
}

struct SyncRange {
    from_operation: OperationID,
    to_operation: OperationID,
    operations: Vec<StoredOperation>,
    hasher: Sha3Hasher,
}

impl SyncRange {
    fn new(from_operation: OperationID, to_operation: OperationID) -> SyncRange {
        SyncRange {
            from_operation,
            to_operation,
            operations: Vec::new(),
            hasher: Sha3Hasher::new_256(),
        }
    }

    fn push_operation(&mut self, operation: StoredOperation) {
        self.to_operation = operation.operation_id;

        let signature_data = operation
            .operation
            .signature_data()
            .expect("One pending operation didn't have signature");
        self.hasher.consume(signature_data);

        self.operations.push(operation);
    }

    fn into_sync_range_frame_builder(
        self,
        requested_details: pending_sync_range::RequestedDetails,
    ) -> Result<FrameBuilder<pending_sync_range::Owned>, Error> {
        let mut range_frame_builder = FrameBuilder::<pending_sync_range::Owned>::new();
        let mut range_msg_builder: pending_sync_range::Builder =
            range_frame_builder.get_builder_typed();

        range_msg_builder.set_from_operation(self.from_operation);
        range_msg_builder.set_to_operation(self.to_operation);
        range_msg_builder.set_operations_count(self.operations.len() as u32);

        // TODO: Support for different level of requested details
        let operations_builder = range_msg_builder
            .reborrow()
            .init_operations(self.operations.len() as u32);
        for (i, operation) in self.operations.iter().enumerate() {
            let operation_frame_reader = operation.operation.get_typed_reader()?;

            // TODO: Make sure it's valid. According to their doc, it may not be right to set a reader if schema of this reader has higher version (and therefor bigger)
            operations_builder.set_with_caveats(i as u32, operation_frame_reader)?;
        }

        let multihash = self.hasher.into_mulithash_bytes();
        range_msg_builder.set_operations_hash(&multihash);

        range_msg_builder.set_requested_details(requested_details);

        Ok(range_frame_builder)
    }
}

///
///
///
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Error in pending store: {:?}", _0)]
    Store(#[fail(cause)] pending::Error),
    #[fail(display = "Error in framing serialization: {:?}", _0)]
    Framing(#[fail(cause)] framed::Error),
    #[fail(display = "Error in capnp serialization: {:?}", _0)]
    CapnpSerialization(capnp::ErrorKind),
}

impl From<pending::Error> for Error {
    fn from(err: pending::Error) -> Self {
        Error::Store(err)
    }
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Framing(err)
    }
}

impl From<capnp::Error> for Error {
    fn from(err: capnp::Error) -> Self {
        Error::CapnpSerialization(err.kind)
    }
}

#[cfg(test)]
mod tests {
    use crate::pending::tests::create_pending_operation;
    use std::sync::Arc;

    use super::*;

    #[test]
    fn sync_ranges_push_operation() {
        let mut sync_ranges = SyncRanges::new(10);
        for operation in operations_generator(90) {
            sync_ranges.push_operation(operation);
        }

        assert_eq!(sync_ranges.ranges.len(), 9);

        // check continuity of ranges
        let mut last_range_to: Option<OperationID> = None;
        for range in sync_ranges.ranges.iter() {
            assert_eq!(range.from_operation, last_range_to.unwrap_or(0));
            last_range_to = Some(range.to_operation);
        }

        assert_eq!(last_range_to, Some((89 + 3) * 13));
    }

    #[test]
    fn sync_ranges_to_frame_builder() {
        let mut sync_ranges = SyncRanges::new(30);
        for operation in operations_generator(90) {
            sync_ranges.push_operation(operation);
        }

        assert_eq!(sync_ranges.ranges.len(), 3);

        let frames_builder: Vec<FrameBuilder<pending_sync_range::Owned>> = sync_ranges
            .ranges
            .into_iter()
            .map(|range| {
                range
                    .into_sync_range_frame_builder(pending_sync_range::RequestedDetails::Hash)
                    .unwrap()
            })
            .collect();

        assert_eq!(frames_builder.len(), 3);

        let frame0 = frames_builder[0].as_owned_unsigned_framed().unwrap();
        let frame0_reader: pending_sync_range::Reader = frame0.get_typed_reader().unwrap();
        let frame0_hash = frame0_reader.reborrow().get_operations_hash().unwrap();

        let frame1 = frames_builder[1].as_owned_unsigned_framed().unwrap();
        let frame1_reader: pending_sync_range::Reader = frame1.get_typed_reader().unwrap();
        let frame1_hash = frame1_reader.reborrow().get_operations_hash().unwrap();

        assert_ne!(frame0_hash, frame1_hash);
    }

    fn operations_generator(count: usize) -> impl Iterator<Item = StoredOperation> {
        (0..count).map(|i| {
            let (group_id, operation_id) = ((i + 2) as u64, ((i + 3) * 13) as u64);
            let operation = Arc::new(create_pending_operation(operation_id, group_id));

            StoredOperation {
                group_id,
                operation_id,
                operation,
            }
        })
    }
}
