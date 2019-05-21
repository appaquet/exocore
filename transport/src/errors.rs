use exocore_common::serialization::{capnp, framed};

/// Transport related error
#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "libp2p transport error: {:?}", _0)]
    Libp2pTransport(Box<dyn std::error::Error + Send + Sync + 'static>),
    #[cfg(feature = "websocket_transport")]
    #[fail(display = "Websocket transport error: {:?}", _0)]
    WebsocketTransport(crate::ws::WebSocketError),
    #[fail(display = "Error in framing serialization: {:?}", _0)]
    Framing(#[fail(cause)] framed::Error),
    #[fail(display = "Error in capnp serialization: kind={:?} msg={}", _0, _1)]
    Serialization(capnp::ErrorKind, String),
    #[fail(display = "Field is not in capnp schema: code={}", _0)]
    SerializationNotInSchema(u16),
    #[fail(display = "Could not upgrade a weak reference")]
    Upgrade,
    #[fail(display = "Try to lock a mutex that was poisoned")]
    Poisoned,
    #[fail(display = "An error occurred: {}", _0)]
    Other(String),
}

impl<Terr> From<libp2p::TransportError<Terr>> for Error
where
    Terr: std::error::Error + Send + Sync + 'static,
{
    fn from(err: libp2p::TransportError<Terr>) -> Self {
        Error::Libp2pTransport(Box::new(err))
    }
}

impl From<crate::ws::WebSocketError> for Error {
    fn from(err: crate::ws::WebSocketError) -> Self {
        Error::WebsocketTransport(err)
    }
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Framing(err)
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

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error::Poisoned
    }
}
