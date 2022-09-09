pub mod common_capnp {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/capn/common_capnp.rs"));
}
pub mod data_chain_capnp {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/capn/data_chain_capnp.rs"));
}
pub mod data_transport_capnp {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/capn/data_transport_capnp.rs"));
}
pub mod store_transport_capnp {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/capn/store_transport_capnp.rs"));
}

pub mod types;
pub use types::*;

pub mod exocore_apps {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/exocore.apps.rs"));
}
pub use self::exocore_apps as apps;

pub mod exocore_core {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/exocore.core.rs"));
}
pub use self::exocore_core as core;

pub mod exocore_store {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/exocore.store.rs"));
}
pub use self::exocore_store as store;

pub mod exocore_test {
    #![allow(clippy::derive_partial_eq_without_eq)]
    include!(concat!(env!("OUT_DIR"), "/exocore.test.rs"));
}
pub use self::exocore_test as test;

pub const STORE_FDSET: &[u8] = include_bytes!("./exocore_store.fd");
pub const TEST_FDSET: &[u8] = include_bytes!("./exocore_test.fd");
