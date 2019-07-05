use crate::error::Error;
use crate::index::EntitiesIndex;
use crate::query::{Query, QueryResults};
use exocore_transport::{InMessage, OutMessage, TransportHandle};
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use std::sync::{Arc, RwLock, Weak};

// TODO: Should get stream from engine handle and trigger re-index
// TODO: Question: should we have a sync store, and then wrap it to make it async ? It would make tests much easier

pub struct Store<CS, PS, T>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    inner: Arc<RwLock<Inner<CS, PS>>>,
    transport_handle: Option<T>,
}

impl<CS, PS, T> Store<CS, PS, T>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    pub fn new(
        data_handle: exocore_data::engine::EngineHandle<CS, PS>,
        index: EntitiesIndex<CS, PS>,
        transport_handle: T,
    ) -> Result<Store<CS, PS, T>, Error> {
        let inner = Arc::new(RwLock::new(Inner {
            index,
            data_handle,
            transport_out: None,
        }));

        Ok(Store {
            inner,
            transport_handle: Some(transport_handle),
        })
    }

    fn start(&mut self) -> Result<(), Error> {
        let mut transport_handle = self
            .transport_handle
            .take()
            .expect("Transport handle was already consumer");

        let mut inner = self.inner.write()?;

        let (out_sender, out_receiver) = mpsc::unbounded();
        tokio::spawn(
            out_receiver
                .forward(transport_handle.get_sink().sink_map_err(|_err| ()))
                .map(|_| ()),
        );
        inner.transport_out = Some(out_sender);

        let weak_inner = Arc::downgrade(&self.inner);
        tokio::spawn(
            transport_handle
                .get_stream()
                .for_each(move |in_message| {
                    if let Err(err) = Self::handle_incoming_message(&weak_inner, in_message) {
                        error!("Error handling incoming message: {}", err);
                        // TODO: Check if we should crash
                    }
                    Ok(())
                })
                .map(|_| ())
                .map_err(|err| {
                    // TODO: Do something ?
                    error!("Error in incoming transport stream: {}", err);
                }),
        );

        tokio::spawn(
            transport_handle
                .map(|_| {
                    info!("Transport is done");
                    // TODO: kill
                })
                .map_err(|err| {
                    error!("Error in transport: {}", err);
                    // TODO: kill
                }),
        );

        let weak_inner = Arc::downgrade(&self.inner);
        tokio::spawn(
            inner
                .data_handle
                .take_events_stream()
                .for_each(move |event| {
                    if let Err(err) = Self::handle_data_event(&weak_inner, event) {
                        error!("Error handling data event: {}", err);
                        // TODO: Check if we should crash
                    }
                    Ok(())
                })
                .map(|_| {
                    info!("Data engine stream is done");
                    // TODO: kill
                })
                .map_err(|err| {
                    error!("Error in data engine stream: {}", err);
                    // TODO: kill
                }),
        );

        Ok(())
    }

    fn handle_incoming_message(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        _in_message: InMessage,
    ) -> Result<(), Error> {
        // TODO: Parse message in query
        //        Check frame type
        //        serde_json...

        // TODO: Proper error handling
        let inner = weak_inner.upgrade().unwrap();
        let inner = inner.read().unwrap();

        let weak_inner = weak_inner.clone();
        tokio::spawn(
            inner
                .handle_incoming_query(Query::Empty)
                .and_then(move |_results| {
                    let inner = weak_inner.upgrade().unwrap();
                    let inner = inner.read().unwrap();
                    if let Some(_transport) = &inner.transport_out {
                        // TODO: let message = OutMessage { to... in_message.source, results ...}
                        //TODO: transport.unbounded_send(message);
                    }

                    Ok(())
                })
                .map_err(|err| {
                    error!("Couldn't run incoming query: {}", err);
                }),
        );

        Ok(())
    }

    fn handle_data_event(
        _weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        event: exocore_data::engine::Event,
    ) -> Result<(), Error> {
        // TODO: Take action... Should we-reindex? ...
        match event {
            exocore_data::engine::Event::ChainBlockNew(_offset) => {
                // TODO: ...
            }
            exocore_data::engine::Event::PendingOperationNew(_op) => {
                // TODO: ...
            }
            _ => {
                // TODO: do all the other
            }
        }

        Ok(())
    }
}

impl<CS, PS, T> Future for Store<CS, PS, T>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        self.start()?;

        // TODO: Return a completion handler instead and wait for stop
        Ok(Async::NotReady)
    }
}

struct Inner<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    index: EntitiesIndex<CS, PS>,
    data_handle: exocore_data::engine::EngineHandle<CS, PS>,
    transport_out: Option<mpsc::UnboundedSender<OutMessage>>, // TODO: Make channel bounded
}

impl<CS, PS> Inner<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    fn handle_incoming_query(&self, _query: Query) -> QueryResolver {
        // TODO: Should use some kind of queue instead of running query directly

        let (sender, receiver) = futures::oneshot();
        let query_resolver = QueryResolver { receiver };

        // TODO: indexer.query(...)

        // TODO: when query ready
        let _ = sender.send(Ok(QueryResults::empty()));

        query_resolver
    }
}

///
///
///
pub struct StoreHandle<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    inner: Weak<RwLock<Inner<CS, PS>>>,
}

///
///
///
///
trait AsyncStore {
    fn query(&self, query: Query) -> Box<dyn Future<Item = QueryResults, Error = Error>>;
}

impl<CS, PS> AsyncStore for StoreHandle<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    fn query(&self, query: Query) -> Box<dyn Future<Item = QueryResults, Error = Error>> {
        // TODO: Proper error handling
        let inner = self.inner.upgrade().unwrap();
        let inner = inner.read().unwrap();
        let resolver = inner.handle_incoming_query(query);
        Box::new(resolver)
    }
}

///
///
///
struct QueryResolver {
    receiver: oneshot::Receiver<Result<QueryResults, Error>>,
}

impl Future for QueryResolver {
    type Item = QueryResults;
    type Error = Error;

    fn poll(&mut self) -> Result<Async<Self::Item>, Self::Error> {
        let result = self
            .receiver
            .poll()
            .map_err(|_err| Error::Other("Query got cancelled".to_string()))?;

        match result {
            Async::Ready(Ok(results)) => Ok(Async::Ready(results)),
            Async::Ready(Err(err)) => Err(err),
            Async::NotReady => Ok(Async::NotReady),
        }
    }
}
