use std::sync::Arc;
use std::unimplemented;

use exocore_protos::generated::store::{EntityQuery, EntityResults};

pub struct Store {

}

impl Store {
    pub async fn query(self: &Arc<Store>, query: EntityQuery) -> Result<EntityResults, StoreError> {
        // TODO: Add to pending requests map
        // TODO: Send to host
        // TODO: Return channel

        unimplemented!()
    }


    pub(crate) fn check_timeouts(&self) {
        // TODO: check if any requests can be considered timed out
    }
}

struct Inner {
    // TODO: Some kind of global pending request map
}

#[derive(Debug, thiserror::Error)]
pub enum StoreError {}

#[no_mangle]
pub extern "C" fn __exocore_store_query_response(reqId: i32, bytes: *const u8, len: usize) {
    // TODO: Find the proper query response channel
    // TODO: Unwrap response
    // TODO: Send on channel
}