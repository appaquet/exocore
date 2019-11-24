pub mod client;
pub use client::{ClientHandle, RemoteStoreClient, RemoteStoreClientConfiguration};

#[cfg(feature = "local_store")]
pub mod server;

#[cfg(test)]
mod tests;
