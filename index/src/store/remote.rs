use crate::domain::schema::Schema;
use crate::error::Error;
use crate::mutation::{Mutation, MutationResult};
use crate::query::{Query, QueryResult};
use crate::store::{AsyncResult, AsyncStore};
use exocore_common::cell::Cell;
use exocore_common::node::Node;
use exocore_common::protos::index_transport_capnp::{mutation_request, query_request};
use exocore_common::protos::MessageType;
use exocore_common::utils::completion_notifier::{
    CompletionError, CompletionListener, CompletionNotifier,
};
use exocore_transport::{InMessage, OutMessage, TransportHandle};
use futures::prelude::*;
use futures::sync::mpsc;
use std::sync::{Arc, RwLock, Weak};

pub type FutureSpawner = Box<dyn Fn(Box<dyn Future<Item = (), Error = ()> + Send>)>;

///
///
///
pub struct RemoteStore<T>
where
    T: TransportHandle,
{
    future_spawner: FutureSpawner,
    start_notifier: CompletionNotifier<(), Error>,
    inner: Arc<RwLock<Inner>>,
    stop_listener: CompletionListener<(), Error>,
    transport_handle: Option<T>,
    started: bool,
}

impl<T> RemoteStore<T>
where
    T: TransportHandle,
{
    pub fn new(
        cell: Cell,
        schema: Arc<Schema>,
        transport_handle: T,
        remote_node: Node,
        future_spawner: FutureSpawner,
    ) -> Result<RemoteStore<T>, Error> {
        let (stop_notifier, stop_listener) = CompletionNotifier::new_with_listener();
        let start_notifier = CompletionNotifier::new();

        let inner = Arc::new(RwLock::new(Inner {
            cell,
            schema,
            transport_out: None,
            remote_node,
            stop_notifier,
        }));

        Ok(RemoteStore {
            future_spawner,
            start_notifier,
            inner,
            stop_listener,
            transport_handle: Some(transport_handle),
            started: false,
        })
    }

    pub fn get_handle(&self) -> Result<StoreHandle, Error> {
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
        let mut transport_handle = self
            .transport_handle
            .take()
            .expect("Transport handle was already consumed");

        let mut inner = self.inner.write()?;

        // send outgoing messages to transport
        let (out_sender, out_receiver) = mpsc::unbounded();
        (self.future_spawner)(Box::new(
            out_receiver
                .forward(transport_handle.get_sink().sink_map_err(|_err| ()))
                .map(|_| ()),
        ));
        inner.transport_out = Some(out_sender);

        // handle incoming messages
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        (self.future_spawner)(Box::new(
            transport_handle
                .get_stream()
                .for_each(move |in_message| {
                    if let Err(err) = Self::handle_incoming_message(&weak_inner1, in_message) {
                        if err.is_fatal() {
                            Inner::notify_stop("incoming message handling", &weak_inner1, Err(err))
                        } else {
                            error!("Couldn't process incoming message: {}", err);
                        }
                    }
                    Ok(())
                })
                .map(|_| ())
                .map_err(move |err| {
                    Inner::notify_stop("incoming transport stream", &weak_inner2, Err(err.into()));
                }),
        ));

        // schedule transport handle
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        (self.future_spawner)(Box::new(
            transport_handle
                .map(move |_| {
                    info!("Transport is done");
                    Inner::notify_stop("transport completion", &weak_inner1, Ok(()));
                })
                .map_err(move |err| {
                    Inner::notify_stop("transport error", &weak_inner2, Err(err.into()));
                }),
        ));

        self.start_notifier.complete(Ok(()));

        Ok(())
    }

    fn handle_incoming_message(
        weak_inner: &Weak<RwLock<Inner>>,
        in_message: InMessage,
    ) -> Result<(), Error> {
        let inner = weak_inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let inner = inner.read()?;

        match message_deserialize_incoming(&in_message, &inner.schema)? {
            IncomingMessage::Mutation(mutation) => {
                // TODO:
            }
            IncomingMessage::Query(query) => {
                // TODO:
            }
        }

        Ok(())
    }
}

impl<T> Future for RemoteStore<T>
where
    T: TransportHandle,
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
struct Inner {
    cell: Cell,
    schema: Arc<Schema>,
    transport_out: Option<mpsc::UnboundedSender<OutMessage>>,
    remote_node: Node,
    stop_notifier: CompletionNotifier<(), Error>,
}

impl Inner {
    fn notify_stop(future_name: &str, weak_inner: &Weak<RwLock<Inner>>, res: Result<(), Error>) {
        match &res {
            Ok(()) => info!("Local store has completed"),
            Err(err) => error!("Got an error in future {}: {}", future_name, err),
        }

        let locked_inner = if let Some(locked_inner) = weak_inner.upgrade() {
            locked_inner
        } else {
            return;
        };

        let inner = if let Ok(inner) = locked_inner.read() {
            inner
        } else {
            return;
        };

        inner.stop_notifier.complete(res);
    }
}

///
/// Parsed incoming message via transport
///
enum IncomingMessage {
    Mutation(Mutation),
    Query(Query),
}

fn message_deserialize_incoming(
    in_message: &InMessage,
    schema: &Arc<Schema>,
) -> Result<IncomingMessage, Error> {
    match in_message.message_type {
        <mutation_request::Owned as MessageType>::MESSAGE_TYPE => {
            let mutation_frame = in_message.get_data_as_framed_message()?;
            let mutation = Mutation::from_mutation_request_frame(schema, mutation_frame)?;
            Ok(IncomingMessage::Mutation(mutation))
        }
        <query_request::Owned as MessageType>::MESSAGE_TYPE => {
            let query_frame = in_message.get_data_as_framed_message()?;
            let query = Query::from_query_request_frame(schema, query_frame)?;
            Ok(IncomingMessage::Query(query))
        }
        other => Err(Error::Other(format!(
            "Received message of unknown type: {}",
            other
        ))),
    }
}

///
/// Async handle to the store
///
pub struct StoreHandle {
    start_listener: CompletionListener<(), Error>,
    inner: Weak<RwLock<Inner>>,
}

impl StoreHandle {
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

impl AsyncStore for StoreHandle {
    fn mutate(&self, mutation: Mutation) -> AsyncResult<MutationResult> {
        unimplemented!()
    }

    fn query(&self, query: Query) -> AsyncResult<QueryResult> {
        unimplemented!()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test() -> Result<(), failure::Error> {
        Ok(())
    }
}
