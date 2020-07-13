use std::time::Duration;

#[derive(Debug, thiserror::Error, Clone)]
pub enum Error {
    #[error("Query parsing error: {0}")]
    QueryParsing(String),

    #[cfg(feature = "local-store")]
    #[error("Error in Tantivy: {0}")]
    Tantivy(std::sync::Arc<tantivy::TantivyError>),

    #[cfg(feature = "local-store")]
    #[error("Error opening Tantivy directory: {0:?}")]
    TantivyOpenDirectoryError(std::sync::Arc<tantivy::directory::error::OpenDirectoryError>),

    #[cfg(feature = "local-store")]
    #[error("Error parsing Tantivy query: {0:?}")]
    TantitvyQueryParsing(std::sync::Arc<tantivy::query::QueryParserError>),

    #[cfg(feature = "local-store")]
    #[error("Chain engine error: {0}")]
    ChainEngine(#[from] exocore_chain::engine::EngineError),

    #[error("Transport error: {0}")]
    Transport(#[from] exocore_transport::Error),

    #[error("Error in capnp serialization: {0}")]
    Serialization(#[from] exocore_core::capnp::Error),

    #[error("Protobuf error: {0}")]
    Proto(#[from] exocore_core::protos::Error),

    #[error("A protobuf field was expected, but was empty: {0}")]
    ProtoFieldExpected(&'static str),

    #[error("IO error of kind {0:?}: {1}")]
    IO(std::io::ErrorKind, String),

    #[error("Error from remote index: {0}")]
    Remote(String),

    #[error("Timeout error: {0:?} > {0:?}")]
    Timeout(Duration, Duration),

    #[error("Try to lock a mutex that was poisoned")]
    Poisoned,

    #[error("Query or mutation got cancelled")]
    Cancelled,

    #[error("Dropped or couldn't get locked")]
    Dropped,

    #[error("Other error occurred: {0}")]
    Other(String),

    #[error("A fatal error occurred: {0}")]
    Fatal(String),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        match self {
            Error::Fatal(_) | Error::Poisoned | Error::Dropped | Error::IO(_, _) => true,

            #[cfg(feature = "local-store")]
            Error::TantivyOpenDirectoryError(_) => true,

            #[cfg(feature = "local-store")]
            Error::ChainEngine(err) if err.is_fatal() => true,

            _ => false,
        }
    }
}

#[cfg(feature = "local-store")]
impl From<tantivy::TantivyError> for Error {
    fn from(err: tantivy::TantivyError) -> Self {
        Error::Tantivy(std::sync::Arc::new(err))
    }
}

#[cfg(feature = "local-store")]
impl From<tantivy::query::QueryParserError> for Error {
    fn from(err: tantivy::query::QueryParserError) -> Self {
        Error::TantitvyQueryParsing(std::sync::Arc::new(err))
    }
}

#[cfg(feature = "local-store")]
impl From<tantivy::directory::error::OpenDirectoryError> for Error {
    fn from(err: tantivy::directory::error::OpenDirectoryError) -> Self {
        Error::TantivyOpenDirectoryError(std::sync::Arc::new(err))
    }
}

impl From<prost::DecodeError> for Error {
    fn from(err: prost::DecodeError) -> Self {
        Error::Proto(err.into())
    }
}

impl From<prost::EncodeError> for Error {
    fn from(err: prost::EncodeError) -> Self {
        Error::Proto(err.into())
    }
}

impl From<std::io::Error> for Error {
    fn from(err: std::io::Error) -> Self {
        Error::IO(err.kind(), err.to_string())
    }
}

impl<T> From<std::sync::PoisonError<T>> for Error {
    fn from(_err: std::sync::PoisonError<T>) -> Self {
        Error::Poisoned
    }
}
