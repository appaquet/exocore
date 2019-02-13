use crate::chain;
use crate::pending;
use crate::transport;

use exocore_common;
use exocore_common::node::Node;
use exocore_common::serialization::framed::TypedFrame;

use futures::prelude::*;
use futures::sync::mpsc;
use futures::sync::oneshot;
use tokio;
use tokio::timer::Interval;

use std;
use std::sync::{Arc, Mutex, RwLock};
use std::time::{Duration, Instant};

// TODO: Should be able to run without the chain.

// TODO: Should have a "EngineState" so that we can easily test the states transition / actions
// TODO: If node has access to data, it needs ot check its integrity by the upper layer
// TODO: If not, a node needs to wait for a majority of nodes that has data
// TODO: should send full if an object has been modified by us recently and we never sent to remote

const ENGINE_MANAGE_TIMER_INTERVAL: Duration = Duration::from_secs(1);

pub struct Engine<T, CS, PS>
where
    T: transport::Transport,
    CS: chain::Store,
    PS: pending::Store,
{
    started: bool,
    transport: T,
    inner: Arc<RwLock<Inner<CS, PS>>>,
    completion_receiver: oneshot::Receiver<Result<(), Error>>,
}

struct Inner<CS, PS>
where
    CS: chain::Store,
    PS: pending::Store,
{
    nodes: Vec<Node>,
    pending_store: Mutex<PS>,
    chain_store: Mutex<CS>,
    transport_sender: Option<mpsc::Sender<transport::OutMessage>>,
    completion_sender: Option<oneshot::Sender<Result<(), Error>>>,
}

impl<T, CS, PS> Engine<T, CS, PS>
where
    T: transport::Transport,
    CS: chain::Store + 'static,
    PS: pending::Store + 'static,
{
    pub fn new(
        transport: T,
        chain_store: CS,
        pending_store: PS,
        nodes: Vec<exocore_common::node::Node>,
    ) -> Engine<T, CS, PS> {
        let (completion_sender, completion_receiver) = oneshot::channel();

        let context = Arc::new(RwLock::new(Inner {
            nodes,
            pending_store: Mutex::new(pending_store),
            chain_store: Mutex::new(chain_store),
            transport_sender: None,
            completion_sender: Some(completion_sender),
        }));

        Engine {
            started: false,
            inner: context,
            transport,
            completion_receiver,
        }
    }

    pub fn get_handle(&mut self) -> Handle {
        // TODO:
        unimplemented!()
    }

    fn start(&mut self) -> Result<(), Error> {
        let transport_in_stream = self.transport.get_stream();
        let transport_out_sink = self.transport.get_sink();

        // handle messages going to transport
        let transport_sender = {
            let weak_inner = Arc::downgrade(&self.inner);
            let (transport_out_channel_sender, transport_out_channel_receiver) = mpsc::unbounded();
            tokio::spawn(
                transport_out_channel_receiver
                    .map_err(|err| {
                        error!("Error in transport_out channel's receiver: {:?}", err);
                        transport::Error::Unknown
                    })
                    .forward(transport_out_sink)
                    .map(|_| ())
                    .map_err(move |err| {
                        error!("Error forwarding to transport sink: {:?}", err);
                        if let Some(locked_inner) = weak_inner.upgrade() {
                            if let Ok(inner) = locked_inner.write().as_mut() {
                                if let Some(sender) = inner.completion_sender.take() {
                                    let _ = sender.send(Err(Error::Unknown));
                                }
                            }
                        }
                    }),
            );
            transport_out_channel_sender
        };

        // handle transport's incoming messages
        {
            let inner = Arc::clone(&self.inner);
            tokio::spawn(
                transport_in_stream
                    .map_err(Error::Transport)
                    .for_each(move |msg: transport::InMessage| {
                        info!("Got incoming message of type: {}", msg.data.message_type());
                        Self::handle_incoming_message(&inner)
                    })
                    .map_err(|err| {
                        // TODO: Mark engine failed
                        error!("Error handling incoming message from transport: {:?}", err);
                    }),
            );
        }

        tokio::spawn({
            Interval::new_interval(ENGINE_MANAGE_TIMER_INTERVAL)
                .for_each(|_| {
                    // TODO: Sync at interval to check we didn't miss anything
                    // TODO: Maybe propose a new block
                    // TODO: Check if transport is complete

                    Ok(())
                })
                .map_err(|err| {
                    // TODO: Mark engine failed
                    error!("Error in management timer: {:?}", err);
                })
        });

        self.started = true;

        Ok(())
    }

    fn handle_incoming_message(inner: &Arc<RwLock<Inner<CS, PS>>>) -> Result<(), Error> {
        unimplemented!()
    }
}

impl<T, CS, PS> Future for Engine<T, CS, PS>
where
    T: transport::Transport,
    CS: chain::Store + 'static,
    PS: pending::Store + 'static,
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
            .map_err(|_cancel| Error::Unknown));

        Ok(Async::Ready(()))
    }
}

#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Try to lock a mutex that was poised")]
    Poisoned,
    #[fail(display = "Error in transport")]
    Transport(transport::Error),
    #[fail(display = "An unknown error occurred")]
    Unknown,
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(err: std::sync::PoisonError<T>) -> Self {
        Error::Poisoned
    }
}

pub struct Handle {}

impl Handle {
    pub fn write_entry(&self) {
        // TODO: Send to pending store
    }

    pub fn get_events_stream(&self, _from_time: Instant)
    /*-> impl futures::Stream<Item = Event, Error = Error>*/
    {
        unimplemented!()
    }
}

///
///
///
struct CommitController {}

enum Event {
    NewPendingTransaction,
    CommitedBlock,
    FrozenBlock, // TODO: x depth
}

struct WrappedEntry {
    status: EntryStatus,
    // either from pending, or chain
}

enum EntryStatus {
    Committed,
    Pending,
}

struct EntryCondition {
    //time_condition
//block_condition
//offset_condition

// TODO: only if it's meant to be in block X at offset Y
}
