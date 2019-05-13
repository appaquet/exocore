use futures::prelude::*;
use libp2p::core::nodes::raw_swarm::ConnectedPoint;
use libp2p::core::swarm::{NetworkBehaviour, NetworkBehaviourAction, PollParameters};
use libp2p::{Multiaddr, PeerId};

use libp2p::core::protocols_handler::{OneShotHandler, ProtocolsHandler};
use std::collections::{HashMap, VecDeque};
use tokio::prelude::{AsyncRead, AsyncWrite};

use super::protocol::{ExocoreProtoConfig, WireMessage};

///
/// Libp2p's behaviour for Exocore. The behaviour defines a protocol that is exposed to
/// lp2p, peers that we want to talk to and acts as a stream / sink of messages exchanged
/// between nodes.
///
pub struct ExocoreBehaviour<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    local_node: PeerId,
    events: VecDeque<NetworkBehaviourAction<WireMessage, ExocoreBehaviourEvent>>,
    peers: HashMap<PeerId, Vec<Multiaddr>>,
    phantom: std::marker::PhantomData<TSubstream>,
}

impl<TSubstream> ExocoreBehaviour<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    pub fn new() -> ExocoreBehaviour<TSubstream> {
        ExocoreBehaviour {
            local_node: PeerId::random(),
            events: VecDeque::new(),
            peers: HashMap::new(),
            phantom: std::marker::PhantomData,
        }
    }

    pub fn send_message(&mut self, peer_id: PeerId, data: Vec<u8>) {
        // TODO: If node is not online, we should queue https://github.com/appaquet/exocore/issues/60
        self.events.push_back(NetworkBehaviourAction::SendEvent {
            peer_id: peer_id.clone(),
            event: WireMessage { data },
        });
    }

    pub fn add_peer(&mut self, peer_id: PeerId, addresses: Vec<Multiaddr>) {
        let current_addresses = self.peers.get(&peer_id);
        if current_addresses.is_none() || current_addresses != Some(&addresses) {
            self.peers.insert(peer_id.clone(), addresses);
            self.events.push_back(NetworkBehaviourAction::DialPeer {
                peer_id: peer_id.clone(),
            });
        }
    }
}

impl<TSubstream> Default for ExocoreBehaviour<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    fn default() -> Self {
        ExocoreBehaviour::new()
    }
}

impl<TSubstream> NetworkBehaviour for ExocoreBehaviour<TSubstream>
where
    TSubstream: AsyncRead + AsyncWrite,
{
    type ProtocolsHandler =
        OneShotHandler<TSubstream, ExocoreProtoConfig, WireMessage, OneshotEvent>;
    type OutEvent = ExocoreBehaviourEvent;

    fn new_handler(&mut self) -> Self::ProtocolsHandler {
        // We use OneShot protocol handler that opens a new stream for every message (stream, not connection)
        Default::default()
    }

    fn addresses_of_peer(&mut self, peer_id: &PeerId) -> Vec<Multiaddr> {
        self.peers.get(peer_id).cloned().unwrap_or_else(Vec::new)
    }

    fn inject_connected(&mut self, peer_id: PeerId, _endpoint: ConnectedPoint) {
        debug!("{}: Connected to {}", self.local_node, peer_id,);
    }

    fn inject_disconnected(&mut self, peer_id: &PeerId, _endpoint: ConnectedPoint) {
        debug!("{}: Disconnected from {}", self.local_node, peer_id,);
        self.events.push_back(NetworkBehaviourAction::DialPeer {
            peer_id: peer_id.clone(),
        });
    }

    fn inject_node_event(&mut self, peer_id: PeerId, event: OneshotEvent) {
        if let OneshotEvent::Received(msg) = event {
            trace!("{}: Received message from {}", self.local_node, peer_id);

            self.events.push_back(NetworkBehaviourAction::GenerateEvent(
                ExocoreBehaviourEvent::Message(ExocoreBehaviourMessage {
                    source: peer_id,
                    data: msg.data,
                }),
            ));
        } else {
            trace!("{}: Our message got sent", self.local_node);
        }
    }

    fn poll(
        &mut self,
        _poll_params: &mut PollParameters<'_>,
    ) -> Async<
        NetworkBehaviourAction<
            <Self::ProtocolsHandler as ProtocolsHandler>::InEvent,
            Self::OutEvent,
        >,
    > {
        if let Some(event) = self.events.pop_front() {
            return Async::Ready(event);
        }

        Async::NotReady
    }
}

///
/// Event emitted by the ExocoreBehaviour
///
#[derive(Debug)]
pub enum ExocoreBehaviourEvent {
    Message(ExocoreBehaviourMessage),
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct ExocoreBehaviourMessage {
    pub source: PeerId,
    pub data: Vec<u8>,
}

///
/// Event emitted by OneShotHandler (protocol handler) when a message has been received
/// or sent.
///
pub enum OneshotEvent {
    Received(WireMessage),
    Sent,
}

impl From<WireMessage> for OneshotEvent {
    #[inline]
    fn from(rpc: WireMessage) -> OneshotEvent {
        OneshotEvent::Received(rpc)
    }
}

impl From<()> for OneshotEvent {
    #[inline]
    fn from(_: ()) -> OneshotEvent {
        OneshotEvent::Sent
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use futures::prelude::*;
    use futures::sync::mpsc;
    use libp2p::core::identity;
    use libp2p::Swarm;
    use libp2p::{Multiaddr, PeerId};
    use tokio::runtime::Runtime;

    use super::*;

    #[test]
    fn behaviour_integration() {
        let mut rt = Runtime::new().unwrap();

        let key1 = identity::Keypair::generate_ed25519();
        let peer1 = PeerId::from(key1.public());
        let transport1 = libp2p::build_development_transport(key1);
        let addr1: Multiaddr = "/ip4/127.0.0.1/tcp/3301".parse().unwrap();

        let behaviour1 = ExocoreBehaviour::new();
        let mut swarm1 = libp2p::core::Swarm::new(transport1, behaviour1, peer1.clone());
        Swarm::listen_on(&mut swarm1, addr1.clone()).unwrap();

        let key2 = identity::Keypair::generate_ed25519();
        let peer2 = PeerId::from(key2.public());
        let transport2 = libp2p::build_development_transport(key2);
        let addr2: Multiaddr = "/ip4/127.0.0.1/tcp/3302".parse().unwrap();

        let behaviour2 = ExocoreBehaviour::new();
        let mut swarm2 = libp2p::core::Swarm::new(transport2, behaviour2, peer2.clone());
        Swarm::listen_on(&mut swarm2, addr2.clone()).unwrap();

        swarm2.add_peer(peer1.clone(), vec![addr1.clone()]);
        swarm1.add_peer(peer2.clone(), vec![addr2.clone()]);

        let (sender1, mut receiver) = mpsc::unbounded::<(PeerId, Vec<u8>)>();
        let mut listening = true;
        rt.spawn(futures::future::poll_fn(move || -> Result<_, ()> {
            while let Async::Ready(Some((peer, data))) =
                receiver.poll().expect("Error polling channel")
            {
                swarm1.send_message(peer, data);
            }

            loop {
                match swarm1.poll().expect("Error while polling swarm") {
                    Async::Ready(Some(data)) => match data {
                        ExocoreBehaviourEvent::Message(msg) => {
                            trace!("Got message from {}", msg.source,);
                        }
                    },
                    Async::Ready(None) | Async::NotReady => {
                        if !listening {
                            if let Some(a) = Swarm::listeners(&swarm1).next() {
                                debug!("Listening on {:?}", a);
                                listening = true;
                            }
                        }
                        break;
                    }
                }
            }

            Ok(Async::NotReady)
        }));

        let (sender2, mut receiver) = mpsc::unbounded::<(PeerId, Vec<u8>)>();
        let mut listening = true;
        rt.spawn(futures::future::poll_fn(move || -> Result<_, ()> {
            while let Async::Ready(Some((peer, data))) =
                receiver.poll().expect("Error polling channel")
            {
                swarm2.send_message(peer, data);
            }

            loop {
                match swarm2.poll().expect("Error while polling swarm") {
                    Async::Ready(Some(data)) => match data {
                        ExocoreBehaviourEvent::Message(msg) => {
                            trace!("Got message from {}", msg.source,);
                        }
                    },
                    Async::Ready(None) | Async::NotReady => {
                        if !listening {
                            if let Some(a) = Swarm::listeners(&swarm2).next() {
                                debug!("Listening on {:?}", a);
                                listening = true;
                            }
                        }
                        break;
                    }
                }
            }

            Ok(Async::NotReady)
        }));

        for i in 0..10 {
            std::thread::sleep(Duration::from_millis(200));
            sender1
                .unbounded_send((peer2.clone(), format!("Data #{}", i).into_bytes()))
                .unwrap();
            sender2
                .unbounded_send((peer1.clone(), format!("Data #{}", i).into_bytes()))
                .unwrap();
        }

        std::thread::sleep(Duration::from_secs(1));
    }
}
