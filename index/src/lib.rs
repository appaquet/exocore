#![deny(bare_trait_objects)]

#[macro_use]
extern crate failure;
#[macro_use]
extern crate serde_derive;
#[macro_use]
extern crate log;
#[macro_use]
extern crate maplit;

pub mod entity_old;
pub mod error;
pub mod index;
pub mod mutation;
pub mod query;
pub mod schema;
pub mod store;
