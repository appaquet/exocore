use crate::payload::PayloadID;
use chrono::{DateTime, Utc};
use futures::lock::Mutex;
use rand::Rng;
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
};

/// Store in which payloads are temporarily saved.
#[derive(Clone, Default)]
pub(super) struct Store {
    inner: Arc<Mutex<StoreInner>>,
    config: super::ServerConfig,
}

impl Store {
    pub(super) fn new(config: super::ServerConfig) -> Store {
        Store {
            inner: Default::default(),
            config,
        }
    }

    pub(super) async fn push(
        &self,
        payload_data: String,
    ) -> Result<(PayloadID, DateTime<Utc>), super::RequestError> {
        let mut inner = self.inner.lock().await;

        if inner.payloads.len() > self.config.max_payloads {
            return Err(super::RequestError::Full);
        }

        let id = inner.next_id();

        let expiration_duration = chrono::Duration::from_std(self.config.expiration)
            .expect("Couldn't convert expiration to chrono Duration");
        let expiration = Utc::now() + expiration_duration;

        inner.payloads.insert(
            id,
            PendingPayload {
                expiration,
                data: payload_data,
            },
        );

        Ok((id, expiration))
    }

    pub(super) async fn get(&self, id: PayloadID) -> Option<String> {
        let mut inner = self.inner.lock().await;
        let payload = inner.payloads.remove(&id)?;
        Some(payload.data)
    }

    pub(super) async fn cleanup(&self) {
        let mut inner = self.inner.lock().await;
        let mut expired = HashSet::new();

        let now = Utc::now();
        for (id, payload) in &inner.payloads {
            if payload.expiration < now {
                expired.insert(*id);
            }
        }

        for id in expired {
            inner.payloads.remove(&id);
        }
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
            // generate a 9 pin random code
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
