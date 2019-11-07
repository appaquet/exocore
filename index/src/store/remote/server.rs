use std::sync::{Arc, RwLock, Weak};

use futures::prelude::*;
use futures::sync::mpsc;

use exocore_common::cell::{Cell, FullCell};
use exocore_common::protos::index_transport_capnp::{mutation_request, query_request};
use exocore_common::protos::MessageType;
use exocore_common::utils::completion_notifier::{
    CompletionError, CompletionListener, CompletionNotifier,
};
use exocore_common::utils::futures::spawn_future;
use exocore_schema::schema::Schema;
use exocore_transport::{InEvent, InMessage, OutEvent, OutMessage, TransportHandle};

use crate::error::Error;
use crate::mutation::{Mutation, MutationResult};
use crate::query::{Query, QueryResult};
use crate::store::AsyncStore;

pub struct StoreServer<CS, PS, T>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    start_notifier: CompletionNotifier<(), Error>,
    started: bool,
    inner: Arc<RwLock<Inner<CS, PS>>>,
    transport_handle: Option<T>,
    stop_listener: CompletionListener<(), Error>,
}

impl<CS, PS, T> StoreServer<CS, PS, T>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
    T: TransportHandle,
{
    pub fn new(
        cell: Cell,
        schema: Arc<Schema>,
        store_handle: crate::store::local::StoreHandle<CS, PS>,
        transport_handle: T,
    ) -> Result<StoreServer<CS, PS, T>, Error> {
        let (stop_notifier, stop_listener) = CompletionNotifier::new_with_listener();
        let start_notifier = CompletionNotifier::new();

        let inner = Arc::new(RwLock::new(Inner {
            cell,
            schema,
            store_handle,
            transport_out: None,
            stop_notifier,
        }));

        Ok(StoreServer {
            start_notifier,
            started: false,
            inner,
            transport_handle: Some(transport_handle),
            stop_listener,
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
        spawn_future(
            out_receiver
                .forward(transport_handle.get_sink().sink_map_err(|_err| ()))
                .map(|_| ()),
        );
        inner.transport_out = Some(out_sender);

        // handle incoming messages
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        spawn_future(
            transport_handle
                .get_stream()
                .map_err(|err| Error::Fatal(format!("Error in incoming transport stream: {}", err)))
                .for_each(move |event| {
                    debug!("Got an incoming message");
                    match event {
                        InEvent::Message(msg) => {
                            if let Err(err) = Self::handle_incoming_message(&weak_inner1, msg) {
                                if err.is_fatal() {
                                    return Err(err);
                                } else {
                                    error!("Couldn't process incoming message: {}", err);
                                }
                            }
                        }
                        InEvent::NodeStatus(_, _) => {
                            // TODO: Do something
                        }
                    }

                    Ok(())
                })
                .map(|_| ())
                .map_err(move |err| {
                    Inner::notify_stop("incoming transport stream", &weak_inner2, Err(err));
                }),
        );

        // schedule transport handle
        let weak_inner1 = Arc::downgrade(&self.inner);
        let weak_inner2 = Arc::downgrade(&self.inner);
        spawn_future(
            transport_handle
                .map(move |_| {
                    info!("Transport is done");
                    Inner::notify_stop("transport completion", &weak_inner1, Ok(()));
                })
                .map_err(move |err| {
                    Inner::notify_stop("transport error", &weak_inner2, Err(err.into()));
                }),
        );

        self.start_notifier.complete(Ok(()));
        info!("Remote store server started");

        Ok(())
    }

    fn handle_incoming_message(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        in_message: Box<InMessage>,
    ) -> Result<(), Error> {
        let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
        let inner = inner.read()?;

        match IncomingMessage::parse_incoming_message(&in_message, &inner.schema)? {
            IncomingMessage::Mutation(mutation) => {
                Self::handle_incoming_mutation_message(weak_inner, in_message, mutation)?;
            }
            IncomingMessage::Query(query) => {
                Self::handle_incoming_query_message(weak_inner, in_message, query)?;
            }
        }

        Ok(())
    }

    fn handle_incoming_query_message(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        in_message: Box<InMessage>,
        query: Query,
    ) -> Result<(), Error> {
        let weak_inner1 = weak_inner.clone();

        let future_result = {
            let inner = weak_inner1.upgrade().ok_or(Error::Dropped)?;
            let inner = inner.read()?;
            inner.store_handle.query(query.clone())
        };

        let weak_inner2 = weak_inner.clone();
        spawn_future(
            future_result
                .then(move |result| {
                    let inner = weak_inner2.upgrade().ok_or(Error::Dropped)?;
                    let inner = inner.read()?;

                    if let Err(err) = &result {
                        error!("Returning error executing incoming query: {}", err);
                    }

                    let resp_frame = QueryResult::result_to_response_frame(&inner.schema, result)?;
                    let message = in_message.to_response_message(&inner.cell, resp_frame)?;
                    inner.send_message(message)?;

                    Ok(())
                })
                .map_err(|err: Error| {
                    error!("Error executing incoming query: {}", err);
                }),
        );

        Ok(())
    }

    fn handle_incoming_mutation_message(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        in_message: Box<InMessage>,
        mutation: Mutation,
    ) -> Result<(), Error> {
        let future_result = {
            let inner = weak_inner.upgrade().ok_or(Error::Dropped)?;
            let inner = inner.read()?;
            inner.store_handle.mutate(mutation)
        };

        let weak_inner1 = weak_inner.clone();
        spawn_future(
            future_result
                .then(move |result| {
                    let inner = weak_inner1.upgrade().ok_or(Error::Dropped)?;
                    let inner = inner.read()?;

                    if let Err(err) = &result {
                        error!("Returning error executing incoming mutation: {}", err);
                    }

                    let resp_frame =
                        MutationResult::result_to_response_frame(&inner.schema, result)?;
                    let message = in_message.to_response_message(&inner.cell, resp_frame)?;
                    inner.send_message(message)?;

                    Ok(())
                })
                .map_err(|err: Error| {
                    error!("Error executing incoming query: {}", err);
                }),
        );

        Ok(())
    }
}

impl<CS, PS, T> Future for StoreServer<CS, PS, T>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
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

struct Inner<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    cell: Cell,
    schema: Arc<Schema>,
    store_handle: crate::store::local::StoreHandle<CS, PS>,
    transport_out: Option<mpsc::UnboundedSender<OutEvent>>,
    stop_notifier: CompletionNotifier<(), Error>,
}

impl<CS, PS> Inner<CS, PS>
where
    CS: exocore_data::chain::ChainStore,
    PS: exocore_data::pending::PendingStore,
{
    fn send_message(&self, message: OutMessage) -> Result<(), Error> {
        let transport = self.transport_out.as_ref().ok_or_else(|| {
            Error::Fatal("Tried to send message, but transport_out was none".to_string())
        })?;

        transport
            .unbounded_send(OutEvent::Message(message))
            .map_err(|_err| {
                Error::Fatal(
                    "Tried to send message, but transport_out channel is closed".to_string(),
                )
            })?;

        Ok(())
    }

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
}

///
/// Parsed incoming message via transport
///
enum IncomingMessage {
    Mutation(Mutation),
    Query(Query),
}

impl IncomingMessage {
    fn parse_incoming_message(
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
}
