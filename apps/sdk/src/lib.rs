pub(crate) mod app;
pub(crate) mod binding;
pub(crate) mod executor;
pub(crate) mod store;

use std::sync::Arc;

pub use app::{App, AppError, __exocore_register_app};
pub use executor::spawn;
pub use exocore_apps_sdk_macro::exocore_app;
pub use store::Store;

#[link(wasm_import_module = "exocore")]
extern "C" {
    // TODO: Should have another name to prevent clashes
    fn log(bytes: *const u8, len: usize);
}

// TODO: Logging
pub(crate) fn send_log(s: &str) {
    unsafe {
        log(s.as_ptr(), s.len());
    }
}

#[derive(Clone)]
pub struct Exocore {
    pub store: Arc<Store>,
}
