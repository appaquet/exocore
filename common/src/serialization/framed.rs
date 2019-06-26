use capnp;

///
/// Trait that needs to have an impl for each capnp generated message struct.
/// Used to identify a unique type id for each message and annotate each framed message.
///
pub trait MessageType<'a>: capnp::traits::Owned<'a> {
    const MESSAGE_TYPE: u16;
}
