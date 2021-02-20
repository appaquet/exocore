use std::sync::Arc;
use std::unimplemented;

use exocore_protos::apps::{OutMessage, OutMessageType};
use exocore_protos::generated::store::{EntityQuery, EntityResults};
use exocore_protos::prost::ProstMessageExt;

use crate::binding::__exocore_host_out_message;

pub struct Store {}

impl Store {
    pub(crate) fn new() -> Store {
        Store {}
    }

    pub async fn query(self: &Arc<Store>, query: EntityQuery) -> Result<EntityResults, StoreError> {
        // TODO: Add to pending requests map
        // TODO: Send to host
        // TODO: Return channel

        let message_type = OutMessageType::OutMsgInvalid as u32;
        let message = OutMessage::default();
        let encoded = message.encode_to_vec();

        unsafe {
            __exocore_host_out_message(message_type as u32, encoded.as_ptr(), encoded.len());
        }

        Err(StoreError::Unknown)
    }

    pub(crate) fn check_timeouts(&self) {
        // TODO: check if any requests can be considered timed out
    }
}

struct Inner {
    // TODO: Some kind of global pending request map
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("Unknown error")]
    Unknown,
}

// #[no_mangle]
// pub extern "C" fn __exocore_store_query_response(reqId: i32, bytes: *const u8, len: usize) {
//     // TODO: Find the proper query response channel
//     // TODO: Unwrap response
//     // TODO: Send on channel
// }
