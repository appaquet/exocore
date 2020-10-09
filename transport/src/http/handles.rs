use std::{collections::HashMap, pin::Pin, sync::Weak, task::Context, task::Poll};

use exocore_core::{
    cell::{Cell, CellId},
    futures::block_on,
    utils::handle_set::Handle,
};
use futures::{channel::mpsc, lock::Mutex, Future, FutureExt};

use crate::{
    streams::{MpscHandleSink, MpscHandleStream},
    transport::TransportHandleOnStart,
    InEvent, InMessage, OutEvent, ServiceType, TransportServiceHandle,
};

use super::server::RequestError;

/// Services registered with the transport that can receive messages and reply
/// to them.
#[derive(Default)]
pub struct ServiceHandles {
    pub(super) services: HashMap<(CellId, ServiceType), ServiceHandle>,
}

impl ServiceHandles {
    pub(super) fn get_handle(
        &mut self,
        cell_id: &CellId,
        layer: ServiceType,
    ) -> Option<&mut ServiceHandle> {
        self.services.get_mut(&(cell_id.clone(), layer))
    }

    fn remove_handle(&mut self, cell_id: &CellId, layer: ServiceType) {
        self.services.remove(&(cell_id.clone(), layer));
    }
}

pub(super) struct ServiceHandle {
    pub(super) cell: Cell,
    pub(super) in_sender: mpsc::Sender<InEvent>,
    pub(super) out_receiver: Option<mpsc::Receiver<OutEvent>>,
}

impl ServiceHandle {
    pub(super) fn send_message(&mut self, msg: Box<InMessage>) -> Result<(), RequestError> {
        self.in_sender
            .try_send(InEvent::Message(msg))
            .map_err(|err| RequestError::Server(format!("Couldn't send to handle: {}", err)))?;

        Ok(())
    }
}

/// Handle to the HTTP transport to be used by a service of a cell.
pub struct HTTPTransportServiceHandle {
    pub(super) cell_id: CellId,
    pub(super) layer: ServiceType,
    pub(super) inner: Weak<Mutex<ServiceHandles>>,
    pub(super) sink: Option<mpsc::Sender<OutEvent>>,
    pub(super) stream: Option<mpsc::Receiver<InEvent>>,
    pub(super) handle: Handle,
}

impl TransportServiceHandle for HTTPTransportServiceHandle {
    type Sink = MpscHandleSink;
    type Stream = MpscHandleStream;

    fn on_started(&self) -> TransportHandleOnStart {
        Box::new(self.handle.on_set_started())
    }

    fn get_sink(&mut self) -> Self::Sink {
        MpscHandleSink::new(self.sink.take().expect("Sink was already consumed"))
    }

    fn get_stream(&mut self) -> Self::Stream {
        MpscHandleStream::new(self.stream.take().expect("Stream was already consumed"))
    }
}

impl Future for HTTPTransportServiceHandle {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.handle.on_set_dropped().poll_unpin(cx)
    }
}

impl Drop for HTTPTransportServiceHandle {
    fn drop(&mut self) {
        debug!(
            "Transport handle for cell {} layer {:?} got dropped. Removing it from transport",
            self.cell_id, self.layer
        );

        // we have been dropped, we remove ourself from layers to communicate with
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = block_on(inner.lock());
            inner.remove_handle(&self.cell_id, self.layer);
        }
    }
}
