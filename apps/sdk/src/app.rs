use super::Exocore;
use std::error::Error;
use std::sync::Mutex;

pub trait App: Send {
    // TODO: Manifest

    fn start(&self, client: &Exocore) -> Result<(), AppError>; // TODO: Context
}

static mut APP: Option<Mutex<Box<dyn App>>> = None;

pub fn __exocore_app_register(app: Box<dyn App>) {
    unsafe {
        APP = Some(Mutex::new(app));
    }
}

#[no_mangle]
pub extern "C" fn __exocore_app_boot() {
    let app = unsafe { APP.as_ref().unwrap().lock().unwrap() };
    app.start(&Exocore::new()).unwrap();
}

#[derive(Debug, thiserror::Error)]
pub enum AppError {
    #[error("Other error: {0:?}")]
    Other(Box<dyn Error>),
}
