use exocore_common::capnp;
use std::sync::Arc;

///
/// Index related error
///
#[derive(Debug, Fail, Clone)]
pub enum Error {
    #[fail(display = "Error parsing schema: {}", _0)]
    Schema(String),
    #[fail(display = "Error in Tantivy: {}", _0)]
    Tantivy(Arc<tantivy::TantivyError>),
    #[fail(display = "Error opening Tantivy directory: {:?}", _0)]
    TantivyOpenDirectoryError(Arc<tantivy::directory::error::OpenDirectoryError>),
    #[fail(display = "Error parsing Tantivy query: {:?}", _0)]
    TantitvyQueryParsing(Arc<tantivy::query::QueryParserError>),
    #[fail(display = "Serde json serialization/deserialization error: {}", _0)]
    SerdeJson(Arc<serde_json::Error>),
    #[fail(display = "Data engine error: {}", _0)]
    DataEngine(#[fail(cause)] exocore_data::engine::Error),
    #[fail(display = "Transport error: {}", _0)]
    Transport(#[fail(cause)] exocore_transport::Error),
    #[fail(display = "Error in capnp serialization: kind={:?} msg={}", _0, _1)]
    Serialization(capnp::ErrorKind, String),
    #[fail(display = "IO error of kind {:?}: {}", _0, _1)]
    IO(std::io::ErrorKind, String),
    #[fail(display = "Try to lock a mutex that was poisoned")]
    Poisoned,
    #[fail(display = "Inner was dropped or couldn't get locked")]
    InnerUpgrade,
    #[fail(display = "Other error occurred: {}", _0)]
    Other(String),
    #[fail(display = "A fatal error occurred: {}", _0)]
    Fatal(String),
}

impl Error {
    pub fn is_fatal(&self) -> bool {
        match self {
            Error::Fatal(_)
            | Error::Poisoned
            | Error::InnerUpgrade
            | Error::IO(_, _)
            | Error::TantivyOpenDirectoryError(_) => true,
            Error::DataEngine(err) if err.is_fatal() => true,
            _ => false,
        }
    }
}

impl From<tantivy::TantivyError> for Error {
    fn from(err: tantivy::TantivyError) -> Self {
        Error::Tantivy(Arc::new(err))
    }
}

impl From<tantivy::query::QueryParserError> for Error {
    fn from(err: tantivy::query::QueryParserError) -> Self {
        Error::TantitvyQueryParsing(Arc::new(err))
    }
}

impl From<tantivy::directory::error::OpenDirectoryError> for Error {
    fn from(err: tantivy::directory::error::OpenDirectoryError) -> Self {
        Error::TantivyOpenDirectoryError(Arc::new(err))
    }
}

impl From<serde_json::Error> for Error {
    fn from(err: serde_json::Error) -> Self {
        Error::SerdeJson(Arc::new(err))
    }
}

impl From<exocore_data::engine::Error> for Error {
    fn from(err: exocore_data::engine::Error) -> Self {
        Error::DataEngine(err)
    }
}

impl From<exocore_transport::Error> for Error {
    fn from(err: exocore_transport::Error) -> Self {
        Error::Transport(err)
    }
}

impl From<capnp::Error> for Error {
    fn from(err: capnp::Error) -> Self {
        Error::Serialization(err.kind, err.description)
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
