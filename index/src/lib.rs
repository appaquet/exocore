#![deny(bare_trait_objects)]

extern crate exocore_common;
extern crate exocore_data;
extern crate exocore_transport;

#[macro_use]
extern crate failure;

extern crate tantivy;

pub mod entity;
pub mod queries;
pub mod store;
