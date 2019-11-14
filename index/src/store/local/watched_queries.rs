use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use futures::sync::mpsc;

use exocore_common::time::Instant;

use crate::error::Error;
use crate::query::{Query, QueryResult, ResultHash, WatchToken};

pub struct WatchedQueries {
    inner: Mutex<Inner>,
}

impl WatchedQueries {
    pub fn new() -> WatchedQueries {
        WatchedQueries {
            inner: Mutex::new(Inner {
                queries: HashMap::new(),
            }),
        }
    }

    pub fn update_query_results(
        &self,
        query: &Query,
        results: &QueryResult,
        sender: Arc<Mutex<mpsc::Sender<Result<QueryResult, Error>>>>,
    ) -> bool {
        let mut inner = self.inner.lock().expect("Inner got poisoned");

        if let Some(token) = query.token {
            if let Some(mut current_watched) = inner.queries.remove(&token) {
                let should_reply = current_watched.last_hash != results.hash;

                current_watched.last_hash = results.hash;
                inner.queries.insert(token, current_watched);

                should_reply
            } else {
                let watched_query = WatchedQuery {
                    token,
                    sender,
                    query: Arc::new(query.clone()),
                    last_register: Instant::now(),
                    last_hash: results.hash,
                };

                inner.queries.insert(token, watched_query);
                true
            }
        } else {
            error!("Query didn't have a watch token");
            true
        }
    }

    pub fn unwatch_query(&self, token: WatchToken) {
        if let Ok(mut inner) = self.inner.lock() {
            inner.queries.remove(&token);
        }
    }

    pub fn queries(&self) -> Vec<WatchedQuery> {
        let inner = self.inner.lock().expect("Inner got poisoned");
        inner.queries.values().cloned().collect()
    }
}

struct Inner {
    queries: HashMap<WatchToken, WatchedQuery>,
}

#[derive(Clone)]
pub struct WatchedQuery {
    pub(crate) token: WatchToken,
    pub(crate) sender: Arc<Mutex<mpsc::Sender<Result<QueryResult, Error>>>>,
    pub(crate) query: Arc<Query>,
    pub(crate) last_register: Instant,
    pub(crate) last_hash: ResultHash,
}
