use bytes::Bytes;
use exocore_core::{
    cell::{Cell, CellId, Node},
    framing::{CapnpFrameBuilder, FrameBuilder, FrameReader, TypedCapnpFrame},
    time::{ConsistentTimestamp, Instant},
};
use exocore_protos::generated::{common_capnp::envelope, MessageType};

use crate::{transport::ConnectionId, Error, ServiceType};

pub type RendezVousId = ConsistentTimestamp;

/// Message to be sent to one or more other nodes.
pub struct OutMessage {
    pub dest_node: Option<Node>,
    pub expiration: Option<Instant>,
    pub connection: Option<ConnectionId>,
    pub envelope_builder: CapnpFrameBuilder<envelope::Owned>,
}

impl OutMessage {
    pub fn from_framed_message<T>(
        cell: &Cell,
        dest_service: ServiceType,
        frame: CapnpFrameBuilder<T>,
    ) -> Result<OutMessage, Error>
    where
        T: for<'a> MessageType<'a>,
    {
        let mut envelope_builder = CapnpFrameBuilder::<envelope::Owned>::new();
        let mut envelope_message_builder = envelope_builder.get_builder();
        envelope_message_builder.set_service(dest_service.to_code());
        envelope_message_builder.set_type(T::MESSAGE_TYPE);
        envelope_message_builder.set_cell_id(cell.id().as_bytes());
        envelope_message_builder.set_from_node_id(&cell.local_node().id().to_string());
        envelope_message_builder.set_data(&frame.as_bytes());

        Ok(OutMessage {
            dest_node: None,
            expiration: None,
            connection: None,
            envelope_builder,
        })
    }

    pub fn with_dest_node(mut self, node: Node) -> Self {
        self.dest_node = Some(node);
        self
    }

    pub fn with_rdv(mut self, rendez_vous_id: RendezVousId) -> Self {
        let mut envelope_message_builder = self.envelope_builder.get_builder();
        envelope_message_builder.set_rendez_vous_id(rendez_vous_id.into());

        self
    }

    pub fn with_expiration(mut self, expiration: Option<Instant>) -> Self {
        self.expiration = expiration;
        self
    }

    pub fn with_connection(self, connection: ConnectionId) -> Self {
        self.with_opt_connection(Some(connection))
    }

    pub fn with_opt_connection(mut self, connection: Option<ConnectionId>) -> Self {
        self.connection = connection;
        self
    }

    #[cfg(any(test, feature = "tests-utils", feature = "http-server"))]
    pub(crate) fn to_in_message(&self, from_node: Node) -> Result<InMessage, Error> {
        let envelope = self.envelope_builder.as_owned_frame();

        let mut msg = InMessage::from_node_and_frame(from_node, envelope)?;
        msg.connection = self.connection.clone();

        Ok(msg)
    }
}

/// Message receive from another node.
pub struct InMessage {
    pub node: Node,
    pub cell_id: CellId,
    pub service_type: ServiceType,
    pub rendez_vous_id: Option<RendezVousId>,
    pub typ: u16,
    pub connection: Option<ConnectionId>,
    pub envelope: TypedCapnpFrame<Bytes, envelope::Owned>,
}

impl InMessage {
    pub fn from_node_and_frame<I: FrameReader<OwnedType = Bytes>>(
        from: Node,
        envelope: TypedCapnpFrame<I, envelope::Owned>,
    ) -> Result<InMessage, Error> {
        let envelope_reader = envelope.get_reader()?;
        let rendez_vous_id = if envelope_reader.get_rendez_vous_id() != 0 {
            Some(envelope_reader.get_rendez_vous_id().into())
        } else {
            None
        };

        let cell_id = CellId::from_bytes(envelope_reader.get_cell_id()?);
        let service_type_id = envelope_reader.get_service();
        let service_type = ServiceType::from_code(service_type_id).ok_or_else(|| {
            Error::Other(format!(
                "Got message with invalid service type id: {}",
                service_type_id
            ))
        })?;

        let message_type = envelope_reader.get_type();

        Ok(InMessage {
            node: from,
            cell_id,
            service_type,
            rendez_vous_id,
            typ: message_type,
            connection: None,
            envelope: envelope.to_owned(),
        })
    }

    pub fn get_data(&self) -> Result<&[u8], Error> {
        let reader = self.envelope.get_reader()?;
        let data = reader.get_data()?;
        Ok(data)
    }

    pub fn get_data_as_framed_message<'d, T>(
        &'d self,
    ) -> Result<TypedCapnpFrame<&'d [u8], T>, Error>
    where
        T: for<'a> MessageType<'a>,
    {
        let reader = self.envelope.get_reader()?;
        let data = reader.get_data()?;
        let frame = TypedCapnpFrame::new(data)?;
        Ok(frame)
    }

    pub fn get_reply_token(&self) -> Result<MessageReplyToken, Error> {
        Ok(MessageReplyToken {
            from: self.node.clone(),
            service_type: self.service_type,
            rendez_vous_id: self.get_rendez_vous_id()?,
            connection: self.connection.clone(),
        })
    }

    pub fn to_response_message<T>(
        &self,
        cell: &Cell,
        frame: CapnpFrameBuilder<T>,
    ) -> Result<OutMessage, Error>
    where
        T: for<'a> MessageType<'a>,
    {
        let reply_token = self.get_reply_token()?;
        reply_token.to_response_message(cell, frame)
    }

    fn get_rendez_vous_id(&self) -> Result<RendezVousId, Error> {
        self.rendez_vous_id.ok_or_else(|| {
            Error::Other(format!(
                "Tried to respond to an InMessage without a follow id (message_type={} service_type={:?})",
                self.typ, self.service_type
            ))
        })
    }
}

/// Structure that contains information that can be used to reply to a received
/// message.
#[derive(Clone)]
pub struct MessageReplyToken {
    from: Node,
    service_type: ServiceType,
    rendez_vous_id: RendezVousId,
    connection: Option<ConnectionId>,
}

impl MessageReplyToken {
    pub fn to_response_message<T>(
        &self,
        cell: &Cell,
        frame: CapnpFrameBuilder<T>,
    ) -> Result<OutMessage, Error>
    where
        T: for<'a> MessageType<'a>,
    {
        Ok(
            OutMessage::from_framed_message(cell, self.service_type, frame)?
                .with_dest_node(self.from.clone())
                .with_rdv(self.rendez_vous_id)
                .with_opt_connection(self.connection.clone()),
        )
    }
}
