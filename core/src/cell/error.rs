#[derive(Debug, Fail)]
pub enum Error {
    #[fail(display = "Configuration error: {}", _0)]
    Cell(String),

    #[fail(display = "Application '{}' error: {}", _0, _1)]
    Application(String, String),

    #[fail(display = "Key error: {}", _0)]
    Key(crate::crypto::keys::Error),

    #[fail(display = "{}", _0)]
    Other(String),
}
