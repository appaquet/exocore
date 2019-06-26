use exocore_common::data_transport_capnp::envelope;
use exocore_common::node::Node;
use exocore_common::serialization::framed::{MessageType, TypedFrame};

use crate::{Error, TransportLayer};
use exocore_common::cell::Cell;
use exocore_common::framing::{CapnpFrameBuilder, FrameBuilder, TypedCapnpFrame};

/// Message to be sent to a another node
pub struct OutMessage {
    pub to: Vec<Node>,
    pub envelope_data: Vec<u8>,
}

impl OutMessage {
    pub fn from_framed_message<R, T>(
        cell: &Cell,
        to_nodes: Vec<Node>,
        frame: R,
    ) -> Result<OutMessage, Error>
    where
        R: TypedFrame<T>,
        T: for<'a> MessageType<'a>,
    {
        let mut envelope_frame_builder = CapnpFrameBuilder::<envelope::Owned>::new();
        let mut envelope_message_builder = envelope_frame_builder.get_builder();
        envelope_message_builder.set_layer(TransportLayer::Data.into());
        envelope_message_builder.set_type(T::MESSAGE_TYPE);
        envelope_message_builder.set_cell_id(cell.id().as_bytes());
        envelope_message_builder.set_from_node_id(&cell.local_node().id().to_str());
        envelope_message_builder.set_data(frame.frame_data());

        Ok(OutMessage {
            to: to_nodes,
            envelope_data: envelope_frame_builder.as_bytes(),
        })
    }

    pub fn to_in_message(&self, from_node: Node) -> InMessage {
        InMessage {
            from: from_node,
            envelope: TypedCapnpFrame::new(self.envelope_data.clone())
                .expect("Couldn't read back envelope frame"),
        }
    }
}

/// Message receive from another node
pub struct InMessage {
    pub from: Node,
    pub envelope: TypedCapnpFrame<Vec<u8>, envelope::Owned>,
}

impl Clone for InMessage {
    fn clone(&self) -> Self {
        InMessage {
            from: self.from.clone(),
            envelope: self.envelope.to_owned(),
        }
    }
}
