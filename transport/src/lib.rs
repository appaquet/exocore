#![deny(bare_trait_objects)]

#[allow(unused_imports)]
#[macro_use]
extern crate log;

pub mod error;
pub mod messages;
pub mod transport;

#[cfg(feature = "libp2p-base")]
#[macro_use]
pub mod p2p;

#[cfg(feature = "libp2p-base")]
pub use p2p::{Libp2pTransport, Libp2pTransportHandle};

#[cfg(any(test, feature = "tests-utils"))]
pub mod mock;

#[cfg(feature = "libp2p-base")]
pub mod either;

pub use error::Error;
pub use messages::{InMessage, OutMessage};
pub use transport::{InEvent, OutEvent, TransportHandle, TransportLayer};
