pub mod client;
pub use client::{Client, ClientConfiguration, ClientHandle};
#[cfg(feature = "local")]
pub mod server;
#[cfg(feature = "local")]
pub use server::{Server, ServerConfiguration};

mod seri;

#[cfg(test)]
mod tests;
