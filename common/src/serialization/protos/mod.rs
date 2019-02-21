// TODO: When Rust IntelliJ will support macro inclusion, we'll get back to the normal way
//pub mod common_capnp {
//    include!(concat!(env!("OUT_DIR"), "/proto/common_capnp.rs"));
//}
//
//pub mod data_chain_capnp {
//    include!(concat!(env!("OUT_DIR"), "/proto/data_chain_capnp.rs"));
//}
//
//pub mod data_transport_capnp {
//    include!(concat!(env!("OUT_DIR"), "/proto/data_transport_capnp.rs"));
//}

use crate::serialization::framed::MessageType;

pub mod common_capnp;
pub mod data_chain_capnp;
pub mod data_transport_capnp;

pub type GroupID = u64;
pub type OperationID = u64;

impl<'a> MessageType<'a> for self::data_chain_capnp::pending_operation::Owned {
    fn message_type() -> u16 {
        0
    }
}

impl<'a> MessageType<'a> for self::data_chain_capnp::entry::Owned {
    fn message_type() -> u16 {
        1
    }
}

impl<'a> MessageType<'a> for self::data_chain_capnp::entry_header::Owned {
    fn message_type() -> u16 {
        2
    }
}

impl<'a> MessageType<'a> for self::data_chain_capnp::block::Owned {
    fn message_type() -> u16 {
        3
    }
}

impl<'a> MessageType<'a> for self::data_chain_capnp::block_signatures::Owned {
    fn message_type() -> u16 {
        4
    }
}

impl<'a> MessageType<'a> for self::data_chain_capnp::block_signature::Owned {
    fn message_type() -> u16 {
        5
    }
}

impl<'a> MessageType<'a> for self::data_transport_capnp::envelope::Owned {
    fn message_type() -> u16 {
        100
    }
}

impl<'a> MessageType<'a> for self::data_transport_capnp::engine_message::Owned {
    fn message_type() -> u16 {
        101
    }
}

impl<'a> MessageType<'a> for self::data_transport_capnp::pending_sync_request::Owned {
    fn message_type() -> u16 {
        102
    }
}

impl<'a> MessageType<'a> for self::data_transport_capnp::pending_sync_response::Owned {
    fn message_type() -> u16 {
        103
    }
}

impl<'a> MessageType<'a> for self::data_transport_capnp::pending_sync_range::Owned {
    fn message_type() -> u16 {
        104
    }
}
