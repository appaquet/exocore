use exocore_common::serialization::framed::{OwnedTypedFrame, TypedFrame};

use exocore_common::serialization::protos::data_chain_capnp::pending_operation;
use exocore_common::serialization::protos::data_transport_capnp::{
    pending_sync_range, pending_sync_request, pending_sync_response,
};
use exocore_common::serialization::protos::OperationID;

use crate::pending;
use exocore_common::serialization::framed::FrameBuilder;
use pending::Store;

pub struct PendingSyncer<PS: Store> {
    // TODO: Cached range hash
    phantom: std::marker::PhantomData<PS>,
}

impl<PS: Store> PendingSyncer<PS> {
    pub fn new() -> PendingSyncer<PS> {
        PendingSyncer {
            phantom: std::marker::PhantomData,
        }
    }

    pub fn create_sync_range_request(
        &self,
        store: &PS,
    ) -> Result<FrameBuilder<pending_sync_request::Owned>, Error> {
        let mut operations_iter = store.operations_iter(..)?;

        if let Some(first_operation) = operations_iter.next() {
            for operation in operations_iter {}

            unimplemented!()
        } else {
            let mut request_frame_builder = FrameBuilder::<pending_sync_request::Owned>::new();
            let mut request_msg_builder: pending_sync_request::Builder =
                request_frame_builder.get_builder_typed();

            let mut range_frame_builder = FrameBuilder::<pending_sync_range::Owned>::new();
            let mut range_msg_builder: pending_sync_range::Builder =
                range_frame_builder.get_builder_typed();
            range_msg_builder.set_from_time(0);
            range_msg_builder.set_to_time(std::u64::MAX);
            range_msg_builder.set_operations_count(0);
            range_msg_builder.set_requested_details(pending_sync_range::RequestedDetails::Hash);

            Ok(request_frame_builder)
        }
    }

    pub fn handle_incoming_sync_request(
        &mut self,
        store: &mut PS,
    ) -> Result<FrameBuilder<pending_sync_response::Owned>, Error> {
        unimplemented!()
    }
}

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Error in pending store: {:?}", _0)]
    Store(#[fail(cause)] pending::Error),
}

impl From<pending::Error> for Error {
    fn from(err: pending::Error) -> Self {
        Error::Store(err)
    }
}

#[cfg(test)]
mod tests {

    #[test]
    fn create_sync_range_request() {}
}
