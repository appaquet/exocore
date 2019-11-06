#![allow(unused)]
#![allow(unused_variables)]

use crate::error::Error;
use crate::query::{Query, QueryResult, ResultHash, WatchToken};
use exocore_common::node::Node;
use exocore_common::time::Instant;
use exocore_transport::messages::RendezVousId;
use futures::sync::mpsc;
use std::collections::HashMap;
use std::env::current_exe;
use std::panic::resume_unwind;
use std::sync::{Arc, Mutex};

// TODO: Debouncing with exponential backoff

pub struct WatchedQueries {
    inner: Mutex<Inner>,
}

#[derive(Clone)]
pub struct WatchedQuery {
    pub(crate) consumer: Consumer,
    pub(crate) query: Arc<Query>,
    pub(crate) last_register: Instant,
    pub(crate) last_hash: ResultHash,
}

#[derive(Clone)]
pub enum Consumer {
    Remote(Arc<Node>, RendezVousId),
    Local(mpsc::UnboundedSender<QueryResult>),
}

impl WatchedQueries {
    pub fn new() -> WatchedQueries {
        WatchedQueries {
            inner: Mutex::new(Inner {
                queries: HashMap::new(),
            }),
        }
    }

    pub fn watch_query(
        &self,
        query: Query,
        consumer: Consumer,
        results: &QueryResult,
    ) -> Result<(), Error> {
        let mut inner = self.inner.lock()?;

        if let Some(token) = query.token {
            inner.queries.insert(
                token,
                WatchedQuery {
                    consumer,
                    query: Arc::new(query),
                    last_register: Instant::now(),
                    last_hash: results.hash,
                },
            );
        }

        Ok(())
    }

    pub fn update_query(&self, query: &Query, results: &QueryResult) -> Result<(), Error> {
        let mut inner = self.inner.lock()?;

        if let Some(token) = query.token {
            if let Some(mut current_watched) = inner.queries.remove(&token) {
                current_watched.last_hash = results.hash;
                inner.queries.insert(token, current_watched);
            }
        }

        Ok(())
    }

    pub fn unwatch_query(&self, token: WatchToken) -> Result<(), Error> {
        let mut inner = self.inner.lock()?;
        inner.queries.remove(&token);
        Ok(())
    }

    pub fn queries(&self) -> Result<Vec<WatchedQuery>, Error> {
        let inner = self.inner.lock()?;
        Ok(inner.queries.values().cloned().collect())
    }
}

struct Inner {
    queries: HashMap<WatchToken, WatchedQuery>,
}
