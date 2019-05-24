#![deny(bare_trait_objects)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;

pub mod entity;
pub mod errors;
pub mod index;
pub mod mutations;
pub mod queries;
pub mod store;
