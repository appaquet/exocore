pub use app::{App, AppError, __exocore_app_register};
pub(crate) mod executor;
pub use executor::spawn;
pub use exocore_apps_sdk_macro::exocore_app;

pub(crate) mod store;
pub use store::Store;

pub mod time;

#[macro_use]
extern crate lazy_static;
use std::sync::Arc;

pub(crate) mod app;
pub(crate) mod binding;
pub(crate) mod log;

pub struct Exocore {
    pub store: Arc<Store>,
}

impl Exocore {
    fn new() -> Exocore {
        Exocore {
            store: Arc::new(Store::new()),
        }
    }
}
