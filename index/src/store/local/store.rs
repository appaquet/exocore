use std::sync::{Arc, RwLock, Weak};

use futures::prelude::*;
use futures::sync::mpsc;
use tokio::prelude::*;

use exocore_common::cell::FullCell;
use exocore_common::protos::index_transport_capnp::{mutation_request, query_request};
use exocore_common::protos::MessageType;
use exocore_common::utils::completion_notifier::{
    CompletionError, CompletionListener, CompletionNotifier,
};
use exocore_common::utils::futures::spawn_future;
use exocore_schema::schema::Schema;
use exocore_transport::{
    InEvent, InMessage, OutEvent, OutMessage, TransportHandle, TransportLayer,
};

use crate::error::Error;
use crate::mutation::{Mutation, MutationResult};
use crate::query::{Query, QueryResult};
use crate::store::local::watched_queries::{Consumer, WatchedQueries};
use crate::store::{AsyncResult, AsyncStore, ResultStream};

use super::entities_index::EntitiesIndex;

// TODO: remove all the remote aspect of this

///
/// Locally persisted store. It uses a data engine handle and entities index to
/// perform mutations and resolve queries
///
pub struct LocalStore<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    start_notifier: CompletionNotifier<(), Error>,
    started: bool,
    inner: Arc<RwLock<Inner<CS, PS>>>,
    //    transport_handle: Option<T>,
    stop_listener: CompletionListener<(), Error>,
}

impl<CS, PS> LocalStore<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    pub fn new(
        cell: FullCell,
        schema: Arc<Schema>,
        data_handle: exocore_data::engine::EngineHandle<CS, PS>,
        index: EntitiesIndex<CS, PS>,
        //        transport_handle: T,
    ) -> Result<LocalStore<CS, PS>, Error> {
        let (stop_notifier, stop_listener) = CompletionNotifier::new_with_listener();
        let start_notifier = CompletionNotifier::new();

        let watched = WatchedQueries::new();

        let inner = Arc::new(RwLock::new(Inner {
            cell,
            schema,
            index,
            watched_queries: watched,
            data_handle,
            transport_out: None,
            stop_notifier,
        }));

        Ok(LocalStore {
            start_notifier,
            started: false,
            inner,
            //            transport_handle: Some(transport_handle),
            stop_listener,
        })
    }

    pub fn get_handle(&self) -> Result<StoreHandle<CS, PS>, Error> {
        let start_listener = self
            .start_notifier
            .get_listener()
            .expect("Couldn't get a listener on start notifier");
        Ok(StoreHandle {
            start_listener,
            inner: Arc::downgrade(&self.inner),
        })
    }

    fn start(&mut self) -> Result<(), Error> {
        let mut inner = self.inner.write()?;

        // query watching changes
        let (mut watching_sender, watching_receiver) = mpsc::channel(2);
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        spawn_future(
            watching_receiver
                .map_err(|_| Error::Dropped)
                // TODO: Throttle
                .for_each(move |_| {
                    let inner = weak_inner1.upgrade().ok_or(Error::Dropped)?;
                    let inner = inner.read()?;

                    // TODO: Should mark as blocking
                    let watched_queries = inner.watched_queries.queries()?;
                    for watched_query in watched_queries {
                        let result = inner.index.search(&watched_query.query)?;
                        if result.hash != watched_query.last_hash {
                            inner
                                .watched_queries
                                .update_query(&watched_query.query, &result)?;

                            match &watched_query.consumer {
                                Consumer::Local(channel) => {
                                    // TODO: Should be bounded
                                    let _ = channel.unbounded_send(result);
                                }
                            }
                        }
                    }

                    Ok(())
                })
                .map_err(move |_| {
                    Inner::notify_stop("query watching stream", &weak_inner2, Err(Error::Dropped))
                }),
        );

        // schedule data engine events stream
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        let weak_inner3 = Arc::downgrade(&self.inner);
        spawn_future(
            inner
                .data_handle
                .take_events_stream()?
                // TODO: throttled
                .map_err(|err| err.into())
                .for_each(move |event| {
                    if let Err(err) = Self::handle_data_engine_event(&weak_inner1, event) {
                        if err.is_fatal() {
                            return Err(err);
                        } else {
                            error!("Error handling data engine event: {}", err);
                        }
                    }

                    // notify query watching. if it's full, it's guaranteed that it will catch those changes on next iteration
                    let _ = watching_sender.try_send(());

                    Ok(())
                })
                .map(move |_| {
                    Inner::notify_stop("data engine event stream completion", &weak_inner2, Ok(()))
                })
                .map_err(move |err| {
                    Inner::notify_stop("data engine event stream", &weak_inner3, Err(err))
                }),
        );

        self.start_notifier.complete(Ok(()));
        info!("Index local store started");

        Ok(())
    }

    fn handle_data_engine_event(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        event: exocore_data::engine::Event,
    ) -> Result<(), Error> {
        let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
        let mut inner = inner.write()?;
        inner.index.handle_data_engine_event(event)?;
        Ok(())
    }
}

impl<CS, PS> Future for LocalStore<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        if !self.started {
            self.start()?;
            self.started = true;
        }

        // check if store got stopped
        self.stop_listener.poll().map_err(|err| match err {
            CompletionError::UserError(err) => err,
            _ => Error::Other("Error in completion error".to_string()),
        })
    }
}

///
/// Inner instance of the store
///
struct Inner<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    cell: FullCell,
    schema: Arc<Schema>,
    index: EntitiesIndex<CS, PS>,
    watched_queries: WatchedQueries,
    data_handle: exocore_data::engine::EngineHandle<CS, PS>,
    transport_out: Option<mpsc::UnboundedSender<OutEvent>>,
    stop_notifier: CompletionNotifier<(), Error>,
}

impl<CS, PS> Inner<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    fn notify_stop(
        future_name: &str,
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        res: Result<(), Error>,
    ) {
        match &res {
            Ok(()) => info!("Local store has completed"),
            Err(err) => error!("Got an error in future {}: {}", future_name, err),
        }

        if let Some(locked_inner) = weak_inner.upgrade() {
            if let Ok(inner) = locked_inner.read() {
                inner.stop_notifier.complete(res);
            }
        };
    }

    fn write_mutation_weak(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        mutation: Mutation,
    ) -> Result<MutationResult, Error> {
        let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
        let inner = inner.read()?;
        inner.write_mutation(mutation)
    }

    fn write_mutation(&self, mutation: Mutation) -> Result<MutationResult, Error> {
        #[cfg(test)]
        {
            if let Mutation::TestFail(_mutation) = &mutation {
                return Err(Error::Other("TestFail mutation".to_string()));
            }
        }

        let json_mutation = mutation.to_json(self.schema.clone())?;
        let operation_id = self
            .data_handle
            .write_entry_operation(json_mutation.as_bytes())?;

        Ok(MutationResult { operation_id })
    }

    fn execute_query_async(
        weak_inner: Weak<RwLock<Inner<CS, PS>>>,
        query: Query,
    ) -> impl Future<Item = QueryResult, Error = Error> {
        // TODO: Use a bounded threadpool instead of executing on current executor: https://github.com/appaquet/exocore/issues/113
        future::lazy(|| {
            future::poll_fn(move || {
                let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
                let inner = inner.read()?;

                let res = tokio_threadpool::blocking(|| inner.index.search(&query));
                match res {
                    Ok(Async::Ready(Ok(results))) => Ok(Async::Ready(results)),
                    Ok(Async::Ready(Err(err))) => Err(err),
                    Ok(Async::NotReady) => Ok(Async::NotReady),
                    Err(err) => Err(Error::Other(format!(
                        "Error executing query in blocking block: {}",
                        err
                    ))),
                }
            })
        })
        .map_err(|err| Error::Other(format!("Error executing query in blocking block: {}", err)))
    }
}

///
/// Handle to the store, allowing communication to the store asynchronously
///
pub struct StoreHandle<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    start_listener: CompletionListener<(), Error>,
    inner: Weak<RwLock<Inner<CS, PS>>>,
}

impl<CS, PS> StoreHandle<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    pub fn on_start(&self) -> Result<impl Future<Item = (), Error = Error>, Error> {
        Ok(self
            .start_listener
            .try_clone()
            .map_err(|_err| Error::Other("Couldn't clone start listener in handle".to_string()))?
            .map_err(|err| match err {
                CompletionError::UserError(err) => err,
                _ => Error::Other("Error in completion error".to_string()),
            }))
    }
}

impl<CS, PS> AsyncStore for StoreHandle<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    fn mutate(&self, mutation: Mutation) -> AsyncResult<MutationResult> {
        Box::new(future::result(Inner::write_mutation_weak(
            &self.inner,
            mutation,
        )))
    }

    fn query(&self, query: Query) -> AsyncResult<QueryResult> {
        let weak_inner = self.inner.clone();
        Box::new(Inner::execute_query_async(weak_inner, query))
    }

    fn watched_query(&self, _query: Query) -> ResultStream<QueryResult> {
        let weak_inner = self.inner.clone();

        // TOOD: Should be bounded
        let (sender, receiver) = mpsc::unbounded();

        Box::new(LocalWatchedQuery {
            inner: weak_inner,
            receiver,
        })
    }
}

pub struct LocalWatchedQuery<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    inner: Weak<RwLock<Inner<CS, PS>>>,
    receiver: mpsc::UnboundedReceiver<QueryResult>,
}

impl<CS, PS> Stream for LocalWatchedQuery<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    type Item = QueryResult;
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Option<Self::Item>>, Self::Error> {
        unimplemented!()
    }
}

impl<CS, PS> Drop for LocalWatchedQuery<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    fn drop(&mut self) {
        // TODO: unregister query
    }
}

#[cfg(test)]
pub mod tests {
    use crate::mutation::TestFailMutation;
    use crate::store::local::TestLocalStore;

    use super::*;

    #[test]
    fn store_mutate_query_via_handle() -> Result<(), failure::Error> {
        let mut test_store = TestLocalStore::new()?;
        test_store.start_store()?;

        let mutation = test_store.create_put_contact_mutation("entry1", "contact1", "Hello World");
        let response = test_store.mutate_via_handle(mutation)?;
        test_store
            .cluster
            .wait_operation_committed(0, response.operation_id);

        let query = Query::match_text("hello");
        let results = test_store.query_via_handle(query)?;
        assert_eq!(results.results.len(), 1);

        Ok(())
    }

    //    #[test]
    //    fn store_mutate_query_via_transport() -> Result<(), failure::Error> {
    //        let mut test_store = TestLocalStore::new()?;
    //        test_store.start_store()?;
    //
    //        let mutation = test_store.create_put_contact_mutation("entry1", "contact1", "Hello World");
    //        let response = test_store.mutate_via_transport(mutation)?;
    //        test_store
    //            .cluster
    //            .wait_operation_committed(0, response.operation_id);
    //
    //        let query = Query::match_text("hello");
    //        let results = test_store.query_via_transport(query)?;
    //        assert_eq!(results.results.len(), 1);
    //
    //        Ok(())
    //    }

    #[test]
    fn query_error_propagating() -> Result<(), failure::Error> {
        let mut test_store = TestLocalStore::new()?;
        test_store.start_store()?;

        let query = Query::test_fail();
        assert!(test_store.query_via_handle(query).is_err());
        //
        //        let query = Query::test_fail();
        //        assert!(test_store.query_via_transport(query).is_err());

        Ok(())
    }

    #[test]
    fn mutation_error_propagating() -> Result<(), failure::Error> {
        let mut test_store = TestLocalStore::new()?;
        test_store.start_store()?;

        let mutation = Mutation::TestFail(TestFailMutation {});
        assert!(test_store.mutate_via_handle(mutation).is_err());
        //
        //        let mutation = Mutation::TestFail(TestFailMutation {});
        //        assert!(test_store.mutate_via_transport(mutation).is_err());

        Ok(())
    }
}
