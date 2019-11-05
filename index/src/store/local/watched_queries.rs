#![allow(unused)]
#![allow(unused_variables)]

use crate::error::Error;
use crate::query::{Query, WatchToken};
use exocore_common::time::Instant;
use std::collections::HashMap;
use std::sync::{Arc, Mutex};

// TODO: Debouncing with exponential backoff

pub struct WatchedQueries {
    inner: Mutex<Inner>,
}

pub struct WatchedQuery {
    query: Query,
    last_register: Instant,
    // TODO: result hash
    // TODO: notifier
}

impl WatchedQueries {
    pub fn new() -> WatchedQueries {
        WatchedQueries {
            inner: Mutex::new(Inner {
                queries: HashMap::new(),
            }),
        }
    }

    // TODO: Should have results
    pub fn watch_query(&self, query: Query) -> Result<(), Error> {
        let mut inner = self.inner.lock()?;

        if let Some(token) = query.token {
            inner.queries.insert(
                token,
                Arc::new(WatchedQuery {
                    query,
                    last_register: Instant::now(),
                }),
            );
        }

        Ok(())
    }

    pub fn unwatch_query(&self, token: WatchToken) -> Result<(), Error> {
        let mut inner = self.inner.lock()?;
        inner.queries.remove(&token);
        Ok(())
    }

    pub fn queries(&self) -> Result<Vec<Arc<WatchedQuery>>, Error> {
        let inner = self.inner.lock()?;
        Ok(inner.queries.values().cloned().collect())
    }
}

struct Inner {
    queries: HashMap<WatchToken, Arc<WatchedQuery>>,
}
