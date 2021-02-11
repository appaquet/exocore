use std::error::Error;
use std::sync::Mutex;
use super::Exocore;

pub trait App: Send {
    // TODO: Manifest

    fn start(&self, client: &Exocore) -> Result<(), AppError>; // TODO: Context
}

static mut APP: Option<Mutex<Box<dyn App>>> = None;

pub fn __exocore_register_app(app: Box<dyn App>) {
    unsafe {
        APP = Some(Mutex::new(app));
    }
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Other error: {0:?}")]
    Other(Box<dyn Error>)
}
