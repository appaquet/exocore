use super::Exocore;
use std::error::Error;
use std::sync::Mutex;

pub trait App: Send {
    fn start(&self, client: &Exocore) -> Result<(), AppError>;
}

static mut APP: Option<Mutex<Box<dyn App>>> = None;

// Called by #[exocore_app] macro at application initialization.
pub fn __exocore_app_register(app: Box<dyn App>) {
    unsafe {
        APP = Some(Mutex::new(app));
    }
}

pub(crate) fn boot_app() {
    let app = unsafe { APP.as_ref().unwrap().lock().unwrap() };
    app.start(&Exocore::new()).unwrap();
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Other error: {0:?}")]
    Other(Box<dyn Error>),
}
