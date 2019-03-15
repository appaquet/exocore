#![allow(dead_code)]

use std;
use std::ops::{Range, RangeBounds};
use std::sync::{Arc, RwLock, Weak};
use std::time::Duration;

use futures::prelude::*;
use futures::sync::mpsc;
use futures::sync::oneshot;
use tokio;
use tokio::timer::Interval;

use exocore_common;
use exocore_common::node::{Node, NodeID, Nodes};
use exocore_common::serialization::capnp;
use exocore_common::serialization::framed::{
    FrameBuilder, MessageType, OwnedTypedFrame, TypedSliceFrame,
};
use exocore_common::serialization::protos::data_chain_capnp::{pending_operation};
use exocore_common::serialization::protos::data_transport_capnp::{
    chain_sync_request, chain_sync_response, envelope, pending_sync_request,
};
use exocore_common::serialization::protos::{GroupID, OperationID};
use exocore_common::serialization::{framed, framed::TypedFrame};

use crate::chain;
use crate::chain::{Block, BlockOffset};
use crate::pending;
use crate::transport;
use crate::transport::OutMessage;

mod chain_sync;
mod commit_manager;
mod pending_sync;
mod request_tracker;

///
/// Data engine's configuration
///
#[derive(Copy, Clone, Debug)]
pub struct Config {
    chain_synchronizer_config: chain_sync::Config,
    pending_synchronizer_config: pending_sync::Config,
    commit_manager_config: commit_manager::Config,
    manager_timer_interval: Duration,
    handles_events_stream_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            chain_synchronizer_config: chain_sync::Config::default(),
            pending_synchronizer_config: pending_sync::Config::default(),
            commit_manager_config: commit_manager::Config::default(),
            manager_timer_interval: Duration::from_secs(1),
            handles_events_stream_size: 1000,
        }
    }
}

///
/// The data engine manages storage and replication of data among the nodes of the cell.
///
/// It contains 2 stores:
///   * Pending store: temporary store in which operations are stored until they get commited to chain
///   * Chain store: persistent store using a block-chain like data structure
///
pub struct Engine<T, CS, PS>
where
    T: transport::Transport,
    CS: chain::Store,
    PS: pending::Store,
{
    config: Config,
    started: bool,
    transport: Option<T>,
    inner: Arc<RwLock<Inner<CS, PS>>>,
    completion_receiver: oneshot::Receiver<Result<(), Error>>,
}

impl<T, CS, PS> Engine<T, CS, PS>
where
    T: transport::Transport,
    CS: chain::Store,
    PS: pending::Store,
{
    pub fn new(
        config: Config,
        node_id: NodeID,
        transport: T,
        chain_store: CS,
        pending_store: PS,
        nodes: Nodes,
    ) -> Engine<T, CS, PS> {
        let (completion_sender, completion_receiver) = oneshot::channel();

        let pending_synchronizer =
            pending_sync::Synchronizer::new(node_id.clone(), config.pending_synchronizer_config);
        let chain_synchronizer =
            chain_sync::Synchronizer::new(node_id.clone(), config.chain_synchronizer_config);
        let commit_manager =
            commit_manager::CommitManager::new(node_id.clone(), config.commit_manager_config);

        let inner = Arc::new(RwLock::new(Inner {
            config,
            node_id,
            nodes,
            pending_store,
            pending_synchronizer,
            chain_store,
            chain_synchronizer,
            commit_manager,
            handles_sender: Vec::new(),
            transport_sender: None,
            completion_sender: Some(completion_sender),
        }));

        Engine {
            config,
            started: false,
            inner,
            transport: Some(transport),
            completion_receiver,
        }
    }

    pub fn get_handle(&mut self) -> Handle<CS, PS> {
        let mut unlocked_inner = self
            .inner
            .write()
            .expect("Inner couldn't get locked, but engine isn't even started yet.");

        let channel_size = self.config.handles_events_stream_size;
        let (events_sender, events_receiver) = mpsc::channel(channel_size);
        unlocked_inner.handles_sender.push(events_sender);

        Handle {
            inner: Arc::downgrade(&self.inner),
            events_receiver: Some(events_receiver),
        }
    }

    fn start(&mut self) -> Result<(), Error> {
        let mut transport = self
            .transport
            .take()
            .ok_or_else(|| Error::Other("Transport was none in engine".to_string()))?;

        let transport_in_stream = transport.get_stream();
        let transport_out_sink = transport.get_sink();

        // create channel to send messages to transport's sink
        {
            let weak_inner = Arc::downgrade(&self.inner);
            let (transport_out_channel_sender, transport_out_channel_receiver) = mpsc::unbounded();
            tokio::spawn(
                transport_out_channel_receiver
                    .map_err(|err| {
                        transport::Error::Other(format!(
                            "Couldn't send to transport_out channel's receiver: {:?}",
                            err
                        ))
                    })
                    .forward(transport_out_sink)
                    .map(|_| ())
                    .map_err(move |err| {
                        Self::handle_spawned_future_error(
                            "transport incoming stream",
                            &weak_inner,
                            Error::Transport(err),
                        );
                    }),
            );

            let mut unlocked_inner = self.inner.write()?;
            unlocked_inner.transport_sender = Some(transport_out_channel_sender);
        }

        // handle transport's incoming messages
        {
            let weak_inner1 = Arc::downgrade(&self.inner);
            let weak_inner2 = Arc::downgrade(&self.inner);
            tokio::spawn(
                transport_in_stream
                    .map_err(Error::Transport)
                    .for_each(move |msg| {
                        Self::handle_incoming_message(&weak_inner1, msg)
                            .or_else(Error::recover_non_fatal_error)
                    })
                    .map_err(move |err| {
                        Self::handle_spawned_future_error(
                            "transport incoming stream",
                            &weak_inner2,
                            err,
                        );
                    }),
            );
        }

        // management timer
        {
            let weak_inner1 = Arc::downgrade(&self.inner);
            let weak_inner2 = Arc::downgrade(&self.inner);
            tokio::spawn({
                Interval::new_interval(self.config.manager_timer_interval)
                    .map_err(|err| Error::Other(format!("Interval error: {:?}", err)))
                    .for_each(move |_| {
                        Self::handle_management_timer_tick(&weak_inner1)
                            .or_else(Error::recover_non_fatal_error)
                    })
                    .map_err(move |err| {
                        Self::handle_spawned_future_error("management timer", &weak_inner2, err);
                    })
            });
        }

        // schedule transport
        {
            let weak_inner1 = Arc::downgrade(&self.inner);
            tokio::spawn({
                transport.map_err(move |err| {
                    Self::handle_spawned_future_error("transport", &weak_inner1, err.into());
                })
            });
        }

        self.started = true;

        info!("Engine started!");
        Ok(())
    }

    fn handle_incoming_message(
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        message: transport::InMessage,
    ) -> Result<(), Error> {
        let locked_inner = weak_inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let mut inner = locked_inner.write()?;

        let envelope_reader: envelope::Reader = message.envelope.get_typed_reader()?;
        debug!(
            "Got message of type {} from node {}",
            envelope_reader.get_type(),
            envelope_reader.get_from_node()?
        );

        match envelope_reader.get_type() {
            <pending_sync_request::Owned as MessageType>::MESSAGE_TYPE => {
                let data = envelope_reader.get_data()?;
                let sync_request = TypedSliceFrame::<pending_sync_request::Owned>::new(data)?;
                inner.handle_incoming_pending_sync_request(&message, sync_request)?;
            }
            <chain_sync_request::Owned as MessageType>::MESSAGE_TYPE => {
                let data = envelope_reader.get_data()?;
                let sync_request = TypedSliceFrame::<chain_sync_request::Owned>::new(data)?;
                inner.handle_incoming_chain_sync_request(&message, sync_request)?;
            }
            <chain_sync_response::Owned as MessageType>::MESSAGE_TYPE => {
                let data = envelope_reader.get_data()?;
                let sync_response = TypedSliceFrame::<chain_sync_response::Owned>::new(data)?;
                inner.handle_incoming_chain_sync_response(&message, sync_response)?;
            }
            msg_type => {
                return Err(Error::Other(format!(
                    "Got an unknown message type: message_type={} transport_layer={}",
                    msg_type,
                    envelope_reader.get_layer()
                )));
            }
        }

        Ok(())
    }

    fn handle_management_timer_tick(weak_inner: &Weak<RwLock<Inner<CS, PS>>>) -> Result<(), Error> {
        let locked_inner = weak_inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let mut inner = locked_inner.write()?;

        inner.tick_synchronizers()?;

        Ok(())
    }

    fn handle_spawned_future_error(
        future_name: &str,
        weak_inner: &Weak<RwLock<Inner<CS, PS>>>,
        error: Error,
    ) {
        error!("Got an error in future {}: {:?}", future_name, error);

        let locked_inner = if let Some(locked_inner) = weak_inner.upgrade() {
            locked_inner
        } else {
            return;
        };

        let mut inner = if let Ok(inner) = locked_inner.write() {
            inner
        } else {
            return;
        };

        inner.try_complete_engine(Err(Error::Other(format!(
            "Couldn't send to completion channel: {:?}",
            error
        ))));
    }
}

impl<T, CS, PS> Future for Engine<T, CS, PS>
where
    T: transport::Transport,
    CS: chain::Store,
    PS: pending::Store,
{
    type Item = ();
    type Error = Error;

    fn poll(&mut self) -> Result<Async<()>, Error> {
        if !self.started {
            self.start()?;
        }

        // check if engine was stopped or failed
        let _ = try_ready!(self
            .completion_receiver
            .poll()
            .map_err(|_cancel| Error::Other("Completion receiver has been cancelled".to_string())));

        Ok(Async::Ready(()))
    }
}

///
///
///
struct Inner<CS, PS>
where
    CS: chain::Store,
    PS: pending::Store,
{
    config: Config,
    node_id: NodeID,
    nodes: Nodes,
    pending_store: PS,
    pending_synchronizer: pending_sync::Synchronizer<PS>,
    chain_store: CS,
    chain_synchronizer: chain_sync::Synchronizer<CS>,
    commit_manager: commit_manager::CommitManager<PS, CS>,
    handles_sender: Vec<mpsc::Sender<Event>>,
    transport_sender: Option<mpsc::UnboundedSender<transport::OutMessage>>,
    completion_sender: Option<oneshot::Sender<Result<(), Error>>>,
}

impl<CS, PS> Inner<CS, PS>
where
    CS: chain::Store,
    PS: pending::Store,
{
    fn handle_new_entry(&mut self, new_entry: NewEntry) -> Result<(), Error> {
        let local_node = self.nodes.get(&self.node_id).ok_or_else(|| {
            Error::Other(format!(
                "Couldn't find local node {} in nodes list",
                self.node_id
            ))
        })?;
        let new_entry_frame = new_entry.into_pending_operation(local_node)?;

        let mut sync_context = SyncContext::new();
        self.pending_synchronizer.handle_new_operation(
            &mut sync_context,
            &self.nodes,
            &mut self.pending_store,
            new_entry_frame,
        )?;

        for message in sync_context.messages {
            let out_message = message.into_out_message(local_node, &self.nodes)?;
            let transport_sender = self.transport_sender.as_ref().expect(
                "Transport sender was none, which mean that the transport was never started",
            );
            transport_sender.unbounded_send(out_message)?;
        }

        Ok(())
    }

    fn handle_incoming_pending_sync_request<R>(
        &mut self,
        message: &transport::InMessage,
        request: R,
    ) -> Result<(), Error>
    where
        R: TypedFrame<pending_sync_request::Owned>,
    {
        let mut sync_context = SyncContext::new();
        self.pending_synchronizer.handle_incoming_sync_request(
            &message.from,
            &mut sync_context,
            &mut self.pending_store,
            request,
        )?;
        self.send_messages_from_sync_context(&mut sync_context)?;
        self.notify_handlers_from_sync_context(&sync_context);

        // TODO: Check events to check if there are any new proposal / signatures that would lead to chain storage

        Ok(())
    }

    fn handle_incoming_chain_sync_request<R>(
        &mut self,
        message: &transport::InMessage,
        request: R,
    ) -> Result<(), Error>
    where
        R: TypedFrame<chain_sync_request::Owned>,
    {
        let mut sync_context = SyncContext::new();
        self.chain_synchronizer.handle_sync_request(
            &mut sync_context,
            &message.from,
            &mut self.chain_store,
            request,
        )?;
        self.send_messages_from_sync_context(&mut sync_context)?;
        self.notify_handlers_from_sync_context(&sync_context);

        Ok(())
    }

    fn handle_incoming_chain_sync_response<R>(
        &mut self,
        message: &transport::InMessage,
        response: R,
    ) -> Result<(), Error>
    where
        R: TypedFrame<chain_sync_response::Owned>,
    {
        let mut sync_context = SyncContext::new();
        self.chain_synchronizer.handle_sync_response(
            &mut sync_context,
            &message.from,
            &mut self.chain_store,
            response,
        )?;
        self.send_messages_from_sync_context(&mut sync_context)?;
        self.notify_handlers_from_sync_context(&sync_context);

        Ok(())
    }

    fn tick_synchronizers(&mut self) -> Result<(), Error> {
        let mut sync_context = SyncContext::new();

        self.chain_synchronizer
            .tick(&mut sync_context, &self.chain_store, &self.nodes)?;
        self.pending_synchronizer
            .tick(&mut sync_context, &self.pending_store, &self.nodes)?;
        self.commit_manager.tick(
            &mut sync_context,
            &mut self.pending_synchronizer,
            &mut self.pending_store,
            &mut self.chain_synchronizer,
            &mut self.chain_store,
            &self.nodes,
        )?;

        self.send_messages_from_sync_context(&mut sync_context)?;
        self.notify_handlers_from_sync_context(&sync_context);

        Ok(())
    }

    fn send_messages_from_sync_context(
        &mut self,
        sync_context: &mut SyncContext,
    ) -> Result<(), Error> {
        if !sync_context.messages.is_empty() {
            let local_node = self.nodes.get(&self.node_id).ok_or_else(|| {
                Error::Other(format!(
                    "Couldn't find local node {} in nodes list",
                    self.node_id
                ))
            })?;

            // swap out messages from the sync_context struct to consume them
            let mut messages = Vec::new();
            std::mem::swap(&mut sync_context.messages, &mut messages);

            for message in messages {
                let out_message = message.into_out_message(local_node, &self.nodes)?;
                let transport_sender = self.transport_sender.as_ref().expect(
                    "Transport sender was none, which mean that the transport was never started",
                );
                transport_sender.unbounded_send(out_message)?;
            }
        }

        Ok(())
    }

    fn notify_handlers_from_sync_context(&mut self, sync_context: &SyncContext) {
        for event in sync_context.events.iter() {
            self.notify_handlers(&event)
        }
    }

    fn notify_handlers(&mut self, event: &Event) {
        for handle_sender in self.handles_sender.iter_mut() {
            if let Err(err) = handle_sender.try_send(event.clone()) {
                // TODO: Put some kind of status on handler so that it knows it lost messages?
                error!(
                    "Couldn't send event to handler. Probably because its queue is full: {:}",
                    err
                );
            }
        }
    }

    fn try_complete_engine(&mut self, result: Result<(), Error>) {
        if let Some(sender) = self.completion_sender.take() {
            let _ = sender.send(result);
        }
    }
}

///
/// Handle ot the Engine, allowing communication with the engine.
/// The engine itself is owned by a future executor.
pub struct Handle<CS, PS>
where
    CS: chain::Store,
    PS: pending::Store,
{
    inner: Weak<RwLock<Inner<CS, PS>>>,
    events_receiver: Option<mpsc::Receiver<Event>>,
}

impl<CS, PS> Handle<CS, PS>
where
    CS: chain::Store,
    PS: pending::Store,
{
    pub fn get_chain_available_segments(&self) -> Result<Vec<Range<chain::BlockOffset>>, Error> {
        // TODO: This should return hashes of last block of each segment

        let inner = self.inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let unlocked_inner = inner.read()?;
        Ok(unlocked_inner.chain_store.available_segments())
    }

    pub fn get_chain_entry(
        &self,
        block_offset: chain::BlockOffset,
        operation_id: OperationID,
    ) -> Result<ChainOperation, Error> {
        let inner = self.inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let unlocked_inner = inner.read()?;

        // TODO: Do a binary lookup after we sorted it
        let block = unlocked_inner.chain_store.get_block(block_offset)?;
        let operation = block
            .entries_iter()?
            .find(|operation| {
                if let Ok(operation_reader) = operation.get_typed_reader() {
                    operation_reader.get_operation_id() == operation_id
                } else {
                    false
                }
            })
            .ok_or_else(|| {
                Error::NotFound(format!(
                    "block_offset={} operation_id={}",
                    block_offset, operation_id
                ))
            })?;

        Ok(ChainOperation {
            operation_id,
            status: EntryStatus::Committed,
            operation_frame: operation.to_owned(),
        })
    }

    pub fn write_entry(&self, entry: NewEntry) -> Result<(), Error> {
        let inner = self.inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let mut unlocked_inner = inner.write()?;
        unlocked_inner.handle_new_entry(entry)
    }

    pub fn get_pending_operations<R: RangeBounds<OperationID>>(
        &self,
        operations_range: R,
    ) -> Result<Vec<pending::StoredOperation>, Error> {
        let inner = self.inner.upgrade().ok_or(Error::InnerUpgrade)?;
        let unlocked_inner = inner.write()?;

        // TODO: how do we eliminate operations that have been committed ?
        //          - add a note in pending store to those that are committed ?

        let operations = unlocked_inner
            .pending_store
            .operations_iter(operations_range)?
            .collect::<Vec<_>>();

        Ok(operations)
    }

    ///
    /// Take the events stream receiver out of this `Handle`.
    /// This stream is bounded and consumptions should be non-blocking to prevent losing events.
    /// Calling the engine on every call should be throttled in the case of a big read amplification.
    pub fn take_events_stream(&mut self) -> impl futures::Stream<Item = Event, Error = Error> {
        self.events_receiver
            .take()
            .expect("Get events stream was already called.")
            .map_err(|_err| Error::Other("Error in incoming events stream".to_string()))
    }
}

impl<CS, PS> Drop for Handle<CS, PS>
where
    CS: chain::Store,
    PS: pending::Store,
{
    fn drop(&mut self) {
        // TODO: Remove our sender from list handlers
    }
}

///
///
///
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Error in transport: {:?}", _0)]
    Transport(#[fail(cause)] transport::Error),
    #[fail(display = "Error in pending store: {:?}", _0)]
    PendingStore(#[fail(cause)] pending::Error),
    #[fail(display = "Error in chain store: {:?}", _0)]
    ChainStore(#[fail(cause)] chain::Error),
    #[fail(display = "Error in pending synchronizer: {:?}", _0)]
    PendingSynchronizer(#[fail(cause)] pending_sync::Error),
    #[fail(display = "Error in chain synchronizer: {:?}", _0)]
    ChainSynchronizer(#[fail(cause)] chain_sync::Error),
    #[fail(display = "Error in commit manager: {:?}", _0)]
    CommitManager(String),
    #[fail(display = "Error in framing serialization: {:?}", _0)]
    Framing(#[fail(cause)] framed::Error),
    #[fail(display = "Error in capnp serialization: kind={:?} msg={}", _0, _1)]
    Serialization(capnp::ErrorKind, String),
    #[fail(display = "Field is not in capnp schema: code={}", _0)]
    SerializationNotInSchema(u16),
    #[fail(display = "Item not found: {}", _0)]
    NotFound(String),
    #[fail(display = "Couldn't send message to a mpsc channel because its receiver was dropped")]
    MpscSendDropped,
    #[fail(display = "Inner was dropped or couldn't get locked")]
    InnerUpgrade,
    #[fail(display = "Try to lock a mutex that was poisoned")]
    Poisoned,
    #[fail(display = "A fatal error occurred: {}", _0)]
    Fatal(String),
    #[fail(display = "An error occurred: {}", _0)]
    Other(String),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        match self {
            Error::PendingStore(inner) => inner.is_fatal(),
            Error::PendingSynchronizer(inner) => inner.is_fatal(),
            Error::ChainStore(inner) => inner.is_fatal(),
            Error::ChainSynchronizer(inner) => inner.is_fatal(),
            Error::MpscSendDropped | Error::InnerUpgrade | Error::Poisoned | Error::Fatal(_) => {
                true
            }
            _ => false,
        }
    }

    fn recover_non_fatal_error(self) -> Result<(), Error> {
        if !self.is_fatal() {
            Ok(())
        } else {
            Err(self)
        }
    }
}

impl From<transport::Error> for Error {
    fn from(err: transport::Error) -> Self {
        Error::Transport(err)
    }
}

impl From<pending::Error> for Error {
    fn from(err: pending::Error) -> Self {
        Error::PendingStore(err)
    }
}

impl From<chain::Error> for Error {
    fn from(err: chain::Error) -> Self {
        Error::ChainStore(err)
    }
}

impl From<pending_sync::Error> for Error {
    fn from(err: pending_sync::Error) -> Self {
        Error::PendingSynchronizer(err)
    }
}

impl From<chain_sync::Error> for Error {
    fn from(err: chain_sync::Error) -> Self {
        Error::ChainSynchronizer(err)
    }
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Framing(err)
    }
}

impl<T> From<mpsc::SendError<T>> for Error {
    fn from(_err: mpsc::SendError<T>) -> Self {
        Error::MpscSendDropped
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error::Poisoned
    }
}

impl From<capnp::Error> for Error {
    fn from(err: capnp::Error) -> Self {
        Error::Serialization(err.kind, err.description)
    }
}

impl From<capnp::NotInSchema> for Error {
    fn from(err: capnp::NotInSchema) -> Self {
        Error::SerializationNotInSchema(err.0)
    }
}

///
///
///
struct SyncContext {
    events: Vec<Event>,
    messages: Vec<SyncContextMessage>,
}

impl SyncContext {
    fn new() -> SyncContext {
        SyncContext {
            events: Vec::new(),
            messages: Vec::new(),
        }
    }

    fn push_pending_sync_request(
        &mut self,
        node_id: NodeID,
        request_builder: FrameBuilder<pending_sync_request::Owned>,
    ) {
        self.messages.push(SyncContextMessage::PendingSyncRequest(
            node_id,
            request_builder,
        ));
    }

    fn push_chain_sync_request(
        &mut self,
        node_id: NodeID,
        request_builder: FrameBuilder<chain_sync_request::Owned>,
    ) {
        self.messages.push(SyncContextMessage::ChainSyncRequest(
            node_id,
            request_builder,
        ));
    }

    fn push_chain_sync_response(
        &mut self,
        node_id: NodeID,
        response_builder: FrameBuilder<chain_sync_response::Owned>,
    ) {
        self.messages.push(SyncContextMessage::ChainSyncResponse(
            node_id,
            response_builder,
        ));
    }
}

enum SyncContextMessage {
    PendingSyncRequest(NodeID, FrameBuilder<pending_sync_request::Owned>),
    ChainSyncRequest(NodeID, FrameBuilder<chain_sync_request::Owned>),
    ChainSyncResponse(NodeID, FrameBuilder<chain_sync_response::Owned>),
}

impl SyncContextMessage {
    fn into_out_message<'n>(
        self,
        local_node: &'n Node,
        nodes: &'n Nodes,
    ) -> Result<OutMessage, Error> {
        let signer = local_node.frame_signer();

        let message = match self {
            SyncContextMessage::PendingSyncRequest(to_node, request_builder) => {
                let to_nodes = nodes.nodes().filter(|n| n.id == to_node).cloned().collect();
                let frame = request_builder.as_owned_framed(signer)?;
                OutMessage::from_framed_message(local_node, to_nodes, frame)?
            }
            SyncContextMessage::ChainSyncRequest(to_node, request_builder) => {
                let to_nodes = nodes.nodes().filter(|n| n.id == to_node).cloned().collect();
                let frame = request_builder.as_owned_framed(signer)?;
                OutMessage::from_framed_message(local_node, to_nodes, frame)?
            }
            SyncContextMessage::ChainSyncResponse(to_node, response_builder) => {
                let to_nodes = nodes.nodes().filter(|n| n.id == to_node).cloned().collect();
                let frame = response_builder.as_owned_framed(signer)?;
                OutMessage::from_framed_message(local_node, to_nodes, frame)?
            }
        };

        Ok(message)
    }
}

///
///
///
#[derive(Clone)]
pub enum Event {
    PendingNew(OperationID),
    PendingRemove(OperationID),
    PendingGroupRemove(GroupID),
    ChainNewBlock(BlockOffset),
    ChainFrozenBlock(BlockOffset), // TODO: x depth
}

pub struct ChainOperation {
    pub operation_id: OperationID,
    pub status: EntryStatus,
    pub operation_frame: OwnedTypedFrame<pending_operation::Owned>,
}

pub enum EntryStatus {
    Committed,
    Pending,
}

pub struct NewEntry {
    pub operation_id: OperationID,
    pub data: Vec<u8>,
}

impl NewEntry {
    pub fn new_cell_data(id: OperationID, data: Vec<u8>) -> NewEntry {
        NewEntry {
            operation_id: id,
            data,
        }
    }

    pub fn into_pending_operation(
        self,
        local_node: &Node,
    ) -> Result<OwnedTypedFrame<pending_operation::Owned>, Error> {
        let mut frame_builder = FrameBuilder::new();
        let mut op_builder: pending_operation::Builder = frame_builder.get_builder_typed();

        op_builder.set_operation_id(self.operation_id);
        op_builder.set_group_id(self.operation_id);

        let inner_op_builder = op_builder.init_operation();

        let mut entry_op_builder = inner_op_builder.init_entry_new();
        entry_op_builder.set_data(&self.data);

        let frame_signer = local_node.frame_signer();
        Ok(frame_builder.as_owned_framed(frame_signer)?)
    }
}

#[cfg(test)]
mod tests {
    use tokio::runtime::Runtime;

    #[test]
    fn engine_get_handle() {
        let _rt = Runtime::new().unwrap();
    }

    #[test]
    fn engine_completion_on_error() {}
}
