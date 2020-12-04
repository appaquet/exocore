#[macro_use]
extern crate log;

pub mod client;
pub mod disco;
pub mod node;
pub mod watched_query;

mod js;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn generate_id(prefix: Option<String>) -> String {
    match prefix.as_deref() {
        Some("") => exocore_core::utils::id::generate_id(),
        Some(prefix) => exocore_core::utils::id::generate_prefixed_id(prefix),
        None => exocore_core::utils::id::generate_id(),
    }
}
