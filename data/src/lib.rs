#![deny(bare_trait_objects)]

extern crate byteorder;
extern crate exocore_common;
#[macro_use]
extern crate failure;
#[macro_use]
extern crate futures;
extern crate itertools;
#[macro_use]
extern crate log;
extern crate memmap;
#[cfg(test)]
extern crate tempdir;
extern crate tokio;
extern crate tokio_io;

///
/// Re-exports
///
pub use crate::chain::directory::{
    Config as ChainDirectoryConfig, DirectoryStore as ChainDirectoryStore,
};
pub use crate::engine::{Config as EngineConfig, Engine, EntryStatus, NewEntry};
pub use crate::pending::memory::MemoryStore as MemoryPendingStore;
pub use crate::transport::mock::{MockTransport, MockTransportHub};

///
/// Modules
///
pub mod chain;
pub mod engine;
pub mod pending;
pub mod transport;
