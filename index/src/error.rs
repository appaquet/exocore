#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Error parsing schema: {}", _0)]
    Schema(String),
    #[fail(display = "An error as occurred in Query parsing: {:?}", _0)]
    QueryParserError(tantivy::query::QueryParserError),
    #[fail(display = "An error as occurred in Tantivy: {}", _0)]
    TantivyError(tantivy::TantivyError),
    #[fail(display = "A file error as occurred.")]
    FileError,
    #[fail(display = "An error occurred: {}", _0)]
    Other(String),
}

impl From<tantivy::query::QueryParserError> for Error {
    fn from(err: tantivy::query::QueryParserError) -> Self {
        Error::QueryParserError(err)
    }
}

impl From<tantivy::TantivyError> for Error {
    fn from(err: tantivy::TantivyError) -> Self {
        Error::TantivyError(err)
    }
}

impl From<std::io::Error> for Error {
    fn from(_: std::io::Error) -> Self {
        Error::FileError
    }
}
