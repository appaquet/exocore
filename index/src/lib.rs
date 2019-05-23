#![deny(bare_trait_objects)]

#[macro_use]
extern crate failure;

pub mod entity;
pub mod errors;
pub mod index;
pub mod mutations;
pub mod queries;
pub mod store;
