pub mod client;
pub use client::{ClientConfiguration, ClientHandle, StoreClient};

#[cfg(feature = "local_store")]
pub mod server;

#[cfg(test)]
mod tests;
