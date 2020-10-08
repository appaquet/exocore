use crate::error::Error;
use crate::transport::TransportHandleOnStart;
use crate::{InEvent, OutEvent, TransportHandle};
use exocore_core::cell::NodeId;
use exocore_core::futures::OwnedSpawnSet;
use futures::channel::mpsc;
use futures::prelude::*;
use futures::{Future, FutureExt, Sink, SinkExt, Stream, StreamExt};
use pin_project::pin_project;
use std::collections::HashMap;
use std::pin::Pin;
use std::sync::{Arc, RwLock, Weak};
use std::task::{Context, Poll};

// TODO: Support connection ID

/// Transport handle that wraps 2 other transport handles. When it receives
/// events, it notes from which transport it came so that replies can be sent
/// back via the same transport.
///
/// Warning: If we never received an event for a node, it will automatically
/// select the first handle !!
#[pin_project]
pub struct EitherTransportHandle<TLeft, TRight>
where
    TLeft: TransportHandle,
    TRight: TransportHandle,
{
    #[pin]
    left: TLeft,
    #[pin]
    right: TRight,
    nodes_side: Arc<RwLock<HashMap<NodeId, Side>>>,
    owned_spawn_set: OwnedSpawnSet<()>,
    completion_sender: mpsc::Sender<()>,
    #[pin]
    completion_receiver: mpsc::Receiver<()>,
}

impl<TLeft, TRight> EitherTransportHandle<TLeft, TRight>
where
    TLeft: TransportHandle,
    TRight: TransportHandle,
{
    pub fn new(left: TLeft, right: TRight) -> EitherTransportHandle<TLeft, TRight> {
        let owned_spawn_set = OwnedSpawnSet::new();
        let (completion_sender, completion_receiver) = mpsc::channel(1);
        EitherTransportHandle {
            left,
            right,
            nodes_side: Arc::new(RwLock::new(HashMap::new())),
            owned_spawn_set,
            completion_sender,
            completion_receiver,
        }
    }

    fn get_side(
        nodes_side: &Weak<RwLock<HashMap<NodeId, Side>>>,
        node_id: &NodeId, // TODO: Add optional connection ID
    ) -> Result<Option<Side>, Error> {
        let nodes_side = nodes_side.upgrade().ok_or(Error::Upgrade)?;
        let nodes_side = nodes_side.read()?;
        Ok(nodes_side.get(node_id).cloned())
    }

    fn maybe_add_node(
        nodes_side: &Weak<RwLock<HashMap<NodeId, Side>>>,
        event: &InEvent,
        side: Side,
    ) -> Result<(), Error> {
        let node_id = match event {
            InEvent::Message(msg) => msg.from.id(),
            _ => return Ok(()),
        };

        let nodes_side = nodes_side.upgrade().ok_or(Error::Upgrade)?;

        {
            // check if node is already in map with same side
            let nodes_side = nodes_side.read()?;
            if nodes_side.get(node_id) == Some(&side) {
                return Ok(());
            }
        }

        {
            // if we're here, node is not in the map or is not on same side
            let mut nodes_side = nodes_side.write()?;
            nodes_side.insert(node_id.clone(), side);
        }

        Ok(())
    }
}

impl<TLeft, TRight> TransportHandle for EitherTransportHandle<TLeft, TRight>
where
    TLeft: TransportHandle,
    TRight: TransportHandle,
{
    type Sink = Box<dyn Sink<OutEvent, Error = Error> + Send + Unpin + 'static>;
    type Stream = Box<dyn Stream<Item = InEvent> + Send + Unpin + 'static>;

    fn on_started(&self) -> TransportHandleOnStart {
        let left = self.left.on_started();
        let right = self.right.on_started();

        Box::new(future::select(left, right).map(|_| ()))
    }

    fn get_sink(&mut self) -> Self::Sink {
        // dispatch incoming events from left transport to handle
        let mut left = self.left.get_sink();
        let (left_sender, mut left_receiver) = mpsc::unbounded();
        let mut completion_sender = self.completion_sender.clone();
        self.owned_spawn_set.spawn(async move {
            while let Some(event) = left_receiver.next().await {
                if let Err(err) = left.send(event).await {
                    error!("Error in sending to left side: {}", err);
                    break;
                }
            }

            let _ = completion_sender.send(()).await;
        });

        // dispatch incoming events from right transport to handle
        let mut right = self.right.get_sink();
        let (right_sender, mut right_receiver) = mpsc::unbounded();
        let mut completion_sender = self.completion_sender.clone();
        self.owned_spawn_set.spawn(async move {
            while let Some(event) = right_receiver.next().await {
                if let Err(err) = right.send(event).await {
                    error!("Error in sending to right side: {}", err);
                    break;
                }
            }

            let _ = completion_sender.send(()).await;
        });

        // redirect outgoing events coming from handles to underlying transports
        let (sender, mut receiver) = mpsc::unbounded::<OutEvent>();
        let nodes_side = Arc::downgrade(&self.nodes_side);
        let mut completion_sender = self.completion_sender.clone();
        self.owned_spawn_set.spawn(
            async move {
                while let Some(event) = receiver.next().await {
                    let side = match &event {
                        OutEvent::Message(msg) => {
                            let node = msg.to.first().ok_or_else(|| {
                                Error::Other(
                                    "Out event didn't have any destination node".to_string(),
                                )
                            })?;
                            Self::get_side(&nodes_side, node.id())? // TODO: Add
                                                                    // connection
                                                                    // ID
                        }
                    };

                    // default to left side if we didn't find node
                    if let Some(Side::Right) = side {
                        right_sender.unbounded_send(event).map_err(|err| {
                            Error::Other(format!("Couldn't send to right sink: {}", err))
                        })?;
                    } else {
                        left_sender.unbounded_send(event).map_err(|err| {
                            Error::Other(format!("Couldn't send to left sink: {}", err))
                        })?;
                    }
                }

                Ok::<_, Error>(())
            }
            .map_err(move |err| {
                error!("Error in sending to either side: {}", err);
                let _ = completion_sender.try_send(());
            })
            .map(|_| ()),
        );

        Box::new(sender.sink_map_err(|err| Error::Other(format!("Error in either sink: {}", err))))
    }

    fn get_stream(&mut self) -> Self::Stream {
        let nodes_side = Arc::downgrade(&self.nodes_side);
        let mut completion_sender = self.completion_sender.clone();
        let left = self.left.get_stream().map(move |event| {
            if let Err(err) = Self::maybe_add_node(&nodes_side, &event, Side::Left) {
                error!("Error saving node's transport from left side: {}", err);
                let _ = completion_sender.try_send(());
            }
            event
        });

        let nodes_side = Arc::downgrade(&self.nodes_side);
        let mut completion_sender = self.completion_sender.clone();
        let right = self.right.get_stream().map(move |event| {
            if let Err(err) = Self::maybe_add_node(&nodes_side, &event, Side::Right) {
                error!("Error saving node's transport from right side: {}", err);
                let _ = completion_sender.try_send(());
            }
            event
        });

        Box::new(futures::stream::select(left, right))
    }
}

impl<TLeft, TRight> Future for EitherTransportHandle<TLeft, TRight>
where
    TLeft: TransportHandle,
    TRight: TransportHandle,
{
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        let mut this = self.project();
        if let Poll::Ready(_) = this.left.poll(cx) {
            return Poll::Ready(());
        }

        if let Poll::Ready(_) = this.right.poll(cx) {
            return Poll::Ready(());
        }

        if let Poll::Ready(_) = this.completion_receiver.next().poll_unpin(cx) {
            return Poll::Ready(());
        }

        Poll::Pending
    }
}

#[derive(PartialEq, Clone, Copy, Debug)]
pub enum Side {
    Left,
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::{MockTransport, TestableTransportHandle};
    use crate::ServiceType::Index;
    use exocore_core::cell::{FullCell, LocalNode};

    // TODO: Test bypass side if connection id

    #[tokio::test]
    async fn test_send_and_receive() -> anyhow::Result<()> {
        let mock1 = MockTransport::default();
        let mock2 = MockTransport::default();

        let node1 = LocalNode::generate();
        let cell1 = FullCell::generate(node1.clone());
        let node2 = LocalNode::generate();
        let cell2 = FullCell::generate(node2.clone());

        // create 2 different kind of transports
        // on node 1, we use it combined using the EitherTransportHandle
        let node1_t1 = mock1.get_transport(node1.clone(), Index);
        let node1_t2 = mock2.get_transport(node1.clone(), Index);
        let mut node1_either = TestableTransportHandle::new(
            EitherTransportHandle::new(node1_t1, node1_t2),
            cell1.cell().clone(),
        );

        // on node 2, we use transports independently
        let node2_t1 = mock1.get_transport(node2.clone(), Index);
        let mut node2_t1 = TestableTransportHandle::new(node2_t1, cell2.cell().clone());
        let node2_t2 = mock2.get_transport(node2.clone(), Index);
        let mut node2_t2 = TestableTransportHandle::new(node2_t2, cell2.cell().clone());

        // since node1 has never sent message, it will send to node 2 via transport 1
        // (left side)
        node1_either.send_rdv(vec![node2.node().clone()], 1).await;
        let _ = node2_t1.recv_msg().await;
        assert_eq!(node2_t2.has_msg().await?, false);

        // sending to node1 via both transport should be received
        node2_t1.send_rdv(vec![node1.node().clone()], 2).await;
        let msg = node1_either.recv_msg().await;
        assert_eq!(msg.from.id(), node2.id());
        assert_eq!(msg.rendez_vous_id, Some(2.into()));
        node2_t2.send_rdv(vec![node1.node().clone()], 3).await;
        let msg = node1_either.recv_msg().await;
        assert_eq!(msg.from.id(), node2.id());
        assert_eq!(msg.rendez_vous_id, Some(3.into()));

        // sending to node2 should now be sent via transport 2 since its last used
        // transport (right side)
        node1_either.send_rdv(vec![node2.node().clone()], 4).await;
        let msg = node2_t2.recv_msg().await;
        assert_eq!(msg.from.id(), node1.id());
        assert_eq!(msg.rendez_vous_id, Some(4.into()));

        Ok(())
    }
}
