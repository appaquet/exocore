use crate::payload::PayloadID;
use chrono::{DateTime, Duration, Utc};
use rand::Rng;
use std::{collections::HashMap, sync::Arc};
use tokio::sync::Mutex;

#[derive(Clone, Default)]
pub struct Store {
    inner: Arc<Mutex<StoreInner>>,
}

impl Store {
    pub async fn push(&self, payload_data: String) -> (PayloadID, DateTime<Utc>) {
        let mut inner = self.inner.lock().await;
        let id = inner.next_id();

        let expiration = Utc::now() + Duration::seconds(30);

        inner.payloads.insert(
            id,
            PendingPayload {
                expiration,
                data: payload_data,
            },
        );

        (id, expiration)
    }

    pub async fn get(&self, id: PayloadID) -> Option<String> {
        let mut inner = self.inner.lock().await;
        let payload = inner.payloads.remove(&id)?;
        Some(payload.data)
    }
}

#[derive(Default)]
struct StoreInner {
    payloads: HashMap<PayloadID, PendingPayload>,
}

impl StoreInner {
    fn next_id(&self) -> PayloadID {
        let mut rng = rand::thread_rng();
        loop {
            let id: PayloadID = rng.gen_range(100_000_000, 999_999_999);
            if !self.payloads.contains_key(&id) {
                return id;
            }
        }
    }
}

pub struct PendingPayload {
    expiration: DateTime<Utc>,
    data: String,
}
