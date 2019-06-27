use crate::entity::Entity;
use crate::errors::Error;
use exocore_transport::{InMessage, OutMessage, TransportHandle};
use futures::prelude::*;
use futures::sync::{mpsc, oneshot};
use std::sync::{Arc, RwLock, Weak};

// TODO: Should get stream from engine handle and trigger re-index
// TODO: Question: should we have a sync store, and then wrap it to make it async ? It would make tests much easier

pub struct Store<CP, PP, T>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    inner: Arc<RwLock<Inner<CP, PP>>>,
    transport_handle: Option<T>,
}

impl<CP, PP, T> Store<CP, PP, T>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    pub fn new(
        data_handle: exocore_data::engine::EngineHandle<CP, PP>,
        transport_handle: T,
    ) -> Store<CP, PP, T> {
        let inner = Arc::new(RwLock::new(Inner {
            data_handle,
            transport_out: None,
        }));

        Store {
            inner,
            transport_handle: Some(transport_handle),
        }
    }

    fn start(&mut self) -> Result<(), Error> {
        let mut transport_handle = self
            .transport_handle
            .take()
            .expect("Transport handle was already consumer");

        // TODO: Proper error handling
        let mut inner = self.inner.write().unwrap();

        let (out_sender, out_receiver) = mpsc::unbounded();
        tokio::spawn(
            out_receiver
                .forward(transport_handle.get_sink().sink_map_err(|err| ()))
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
        weak_inner: &Weak<RwLock<Inner<CP, PP>>>,
        in_message: InMessage,
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
                .and_then(move |results| {
                    let inner = weak_inner.upgrade().unwrap();
                    let inner = inner.read().unwrap();
                    if let Some(transport) = &inner.transport_out {
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
        weak_inner: &Weak<RwLock<Inner<CP, PP>>>,
        event: exocore_data::engine::Event,
    ) -> Result<(), Error> {
        // TODO: Take action... Should we-reindex? ...
        match event {
            exocore_data::engine::Event::ChainBlockNew(offset) => {
                // TODO: ...
            }
            exocore_data::engine::Event::PendingOperationNew(op) => {
                // TODO: ...
            }
            _ => {
                // TODO: do all the other
            }
        }

        Ok(())
    }
}

impl<CP, PP, T> Future for Store<CP, PP, T>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
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

struct Inner<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    //TODO: index: crate::index::Index,
    data_handle: exocore_data::engine::EngineHandle<CP, PP>,
    transport_out: Option<mpsc::UnboundedSender<OutMessage>>, // TODO: Make channel bounded
}

impl<CP, PP> Inner<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    fn handle_incoming_query(&self, query: Query) -> QueryResolver {
        // TODO: Should use some kind of queue instead of running query directly

        let (sender, receiver) = futures::oneshot();
        let query_resolver = QueryResolver { receiver };

        // TODO: indexer.query(...)

        // TODO: when query ready
        sender.send(Ok(QueryResults::empty()));

        query_resolver
    }
}

///
///
///
struct StoreHandle<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
{
    inner: Weak<RwLock<Inner<CP, PP>>>,
}

///
///
///
///
trait AsyncStore {
    fn query(&self, query: Query) -> Box<dyn Future<Item = QueryResults, Error = Error>>;
}

impl<CP, PP> AsyncStore for StoreHandle<CP, PP>
where
    CP: exocore_data::chain::ChainStore,
    PP: exocore_data::pending::PendingStore,
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

///
///
///
#[serde(rename_all = "snake_case", tag = "type")]
#[derive(Serialize, Deserialize)]
pub enum Query {
    WithTrait(WithTraitQuery),
    Conjunction(ConjunctionQuery),
    Match(MatchQuery),

    // TODO: just for tests for now
    Empty,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct WithTraitQuery {
    trait_name: String,
    trait_query: Option<Box<Query>>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct ConjunctionQuery {
    queries: Vec<Query>,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct MatchQuery {
    query: String,
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct SortToken(pub String);

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryPaging {
    from_token: Option<SortToken>,
    to_token: Option<SortToken>,
    count: u32,
}
impl QueryPaging {
    fn empty() -> QueryPaging {
        QueryPaging {
            from_token: None,
            to_token: None,
            count: 0,
        }
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryResults {
    results: Vec<QueryResult>,
    total_estimated: u32,
    current_page: QueryPaging,
    next_page: Option<QueryPaging>,
    // TODO: currentPage, nextPage, queryToken
}

impl QueryResults {
    fn empty() -> QueryResults {
        QueryResults {
            results: vec![],
            total_estimated: 0,
            current_page: QueryPaging::empty(),
            next_page: None,
        }
    }
}

#[serde(rename_all = "snake_case")]
#[derive(Serialize, Deserialize)]
pub struct QueryResult {
    entity: Entity,
    // TODO: sortToken:
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_serialization() -> Result<(), failure::Error> {
        let query = Query::WithTrait(WithTraitQuery {
            trait_name: "trait".to_string(),
            trait_query: None,
        });
        let serialized = serde_json::to_string(&query)?;
        println!("{}", serialized);

        let entity = Entity {
            id: 0,
            creation_date: 0,
            modification_date: 0,
            traits: vec![],
        };
        let results = QueryResults {
            results: vec![QueryResult { entity }],
            total_estimated: 0,
            current_page: QueryPaging {
                from_token: Some(SortToken("token".to_string())),
                to_token: None,
                count: 10,
            },
            next_page: None,
        };
        let serialized = serde_json::to_string(&results)?;
        println!("{}", serialized);

        Ok(())
    }

}
