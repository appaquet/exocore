use crate::OutMessage;
use exocore_core::futures::{block_on, delay_for};
use futures::{channel::oneshot, lock::Mutex, FutureExt};
use std::{
    collections::HashMap, sync::atomic::AtomicU64, sync::atomic::Ordering, sync::Arc, sync::Weak,
    time::Duration,
};

use super::config::HTTPTransportConfig;

pub type RequestID = u64;

pub struct Requests {
    requests: Mutex<HashMap<RequestID, oneshot::Sender<OutMessage>>>,
    next_id: AtomicU64,
    config: HTTPTransportConfig,
}

impl Requests {
    pub fn new(config: HTTPTransportConfig) -> Requests {
        Requests {
            requests: Mutex::new(HashMap::new()),
            next_id: AtomicU64::new(0),
            config,
        }
    }

    pub async fn push(self: Arc<Self>) -> Request {
        let mut requests = self.requests.lock().await;

        let id = self.next_id.fetch_add(1, Ordering::SeqCst);

        let (sender, receiver) = oneshot::channel();
        let request = Request {
            id,
            requests: Arc::downgrade(&self),
            receiver: Some(receiver),
            receive_timeout: self.config.request_timeout,
        };

        requests.insert(request.id, sender);

        request
    }

    pub async fn reply(&self, request_id: RequestID, message: OutMessage) {
        let sender = {
            let mut requests = self.requests.lock().await;
            requests.remove(&request_id)
        };

        if let Some(sender) = sender {
            if sender.send(message).is_err() {
                warn!(
                    "Error replying message to request {}. Channel got dropped.",
                    request_id
                );
            }
        } else {
            warn!(
                "Tried to reply to request {}, but wasn't there anymore (timed-out?)",
                request_id
            );
        }
    }

    pub async fn remove(&self, request_id: RequestID) {
        let mut requests = self.requests.lock().await;
        requests.remove(&request_id);
    }
}

pub struct Request {
    id: RequestID,
    requests: Weak<Requests>,
    receiver: Option<oneshot::Receiver<OutMessage>>,
    receive_timeout: Duration,
}

impl Request {
    pub fn id(&self) -> RequestID {
        self.id
    }

    pub async fn get_response_or_timeout(mut self) -> Result<OutMessage, ()> {
        let receiver = self.receiver.take().ok_or(())?;
        let timeout = delay_for(self.receive_timeout);

        futures::select! {
            resp = receiver.fuse() => {
                resp.map_err(|_| ())
            },
            _ = timeout.fuse() => {
                Err(())
            },
        }
    }
}

impl Drop for Request {
    fn drop(&mut self) {
        if let Some(requests) = self.requests.upgrade() {
            block_on(requests.remove(self.id));
        }
    }
}
