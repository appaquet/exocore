#![deny(bare_trait_objects)]

#[macro_use]
extern crate log;

mod js;
mod ws;

pub mod client;
pub mod watched_query;

use wasm_bindgen::prelude::*;

#[wasm_bindgen]
pub fn generate_id(prefix: Option<String>) -> String {
    match prefix {
        Some(prefix) => exocore_common::utils::id::generate_prefixed_id(&prefix),
        None => exocore_common::utils::id::generate_id(),
    }
}
