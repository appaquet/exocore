use std::{
    borrow::Cow,
    collections::HashMap,
    pin::Pin,
    sync::{Arc, Weak},
    task::Context,
    task::Poll,
};

use exocore_core::{
    capnp,
    cell::{Cell, CellId, CellNodes, Node},
    crypto::auth_token::AuthToken,
    framing::{CapnpFrameBuilder, FrameBuilder},
    futures::block_on,
    protos::generated::index_transport_capnp::query_request,
    protos::generated::index_transport_capnp::query_response,
    time::Clock,
    utils::handle_set::Handle,
    utils::handle_set::HandleSet,
};
use futures::{channel::mpsc, lock::Mutex, Future, FutureExt, StreamExt};
use hyper::{
    service::{make_service_fn, service_fn},
    StatusCode,
};
use hyper::{Body, Request, Response, Server};

use crate::Error;
use crate::{
    streams::{MpscHandleSink, MpscHandleStream},
    transport::ConnectionID,
    transport::TransportHandleOnStart,
    InEvent, InMessage, OutEvent, OutMessage, TransportHandle, TransportLayer,
};

pub mod config;
use config::HTTPTransportConfig;

mod requests;
pub use requests::RequestID;
use requests::{RequestTracker, TrackedRequest};

/// Unidirectional HTTP transport server used for request-response type of communication by clients
/// for which a full libp2p transport is impossible.
///
/// Since it doesn't run a full fledge transport, authentication is achieved through a generated `AuthToken`
/// signed by the public key of a node of the cell.
///
/// At the moment, this transport is only used for entity queries and mutations.
pub struct HTTPTransportServer {
    config: HTTPTransportConfig,
    clock: Clock,
    services: Arc<Mutex<Services>>,
    handle_set: HandleSet,
}

impl HTTPTransportServer {
    /// Creates a new HTTP server with the given configuration and clock.
    pub fn new(config: HTTPTransportConfig, clock: Clock) -> HTTPTransportServer {
        HTTPTransportServer {
            config,
            clock,
            services: Default::default(),
            handle_set: Default::default(),
        }
    }

    /// Get a transport handle that will be used by services. This handle can only be used to receive
    /// messages and reply to them.
    pub fn get_handle(
        &mut self,
        cell: Cell,
        layer: TransportLayer,
    ) -> Result<HTTPTransportServiceHandle, Error> {
        let (in_sender, in_receiver) = mpsc::channel(self.config.handle_in_channel_size);
        let (out_sender, out_receiver) = mpsc::channel(self.config.handle_out_channel_size);

        // Register new handle and its streams
        let mut handles = block_on(self.services.lock());
        let inner_layer = ServiceChannels {
            cell: cell.clone(),
            in_sender,
            out_receiver: Some(out_receiver),
        };
        info!(
            "Registering transport for cell {} and layer {:?}",
            cell, layer
        );
        let key = (cell.id().clone(), layer);
        handles.services.insert(key, inner_layer);

        Ok(HTTPTransportServiceHandle {
            cell_id: cell.id().clone(),
            layer,
            inner: Arc::downgrade(&self.services),
            sink: Some(out_sender),
            stream: Some(in_receiver),
            handle: self.handle_set.get_handle(),
        })
    }

    /// Runs the HTTP server and returns when it's done.
    pub async fn run(self) -> Result<(), Error> {
        let request_tracker = Arc::new(RequestTracker::new(self.config.clone()));

        // Listen on all addresess
        let servers = {
            let mut futures = Vec::new();
            for addr in &self.config.listen_addresses {
                let request_tracker = request_tracker.clone();
                let services = self.services.clone();
                let clock = self.clock.clone();

                let server = Server::bind(&addr).serve(make_service_fn(move |_socket| {
                    let request_tracker = request_tracker.clone();
                    let services = services.clone();
                    let clock = clock.clone();
                    async move {
                        Ok::<_, hyper::Error>(service_fn(move |req| {
                            let request_tracker = request_tracker.clone();
                            let services = services.clone();
                            let clock = clock.clone();

                            async {
                                let resp =
                                    handle_request(request_tracker, services, clock, req).await;

                                let resp = match resp {
                                    Ok(resp) => resp,
                                    Err(err) => {
                                        error!("Error handling request: {}", err);
                                        err.to_response()
                                    }
                                };

                                Ok::<_, hyper::Error>(resp)
                            }
                        }))
                    }
                }));

                futures.push(server);
            }

            futures::future::join_all(futures)
        };

        // Takes care of outgoing messages from services to be dispatched to connections
        let handles_dispatcher = {
            let services = self.services.clone();
            let request_tracker = request_tracker.clone();

            async move {
                let mut inner = services.lock().await;

                let mut futures = Vec::new();
                for service_channels in inner.services.values_mut() {
                    let mut out_receiver = service_channels
                        .out_receiver
                        .take()
                        .expect("Out receiver of one service was already consumed");

                    let connections = request_tracker.clone();
                    futures.push(async move {
                        while let Some(event) = out_receiver.next().await {
                            let  OutEvent::Message(message) = event;
                            let connection_id = match message.connection {
                                Some(ConnectionID::HTTPServer(id)) => id,
                                _ => {
                                    warn!("Couldn't find connection id in message to be send back to connection");
                                    continue;
                                }
                            };

                            connections.reply(connection_id, message).await;
                        }
                    });
                }
                futures::future::join_all(futures)
            }
            .await
        };

        info!("HTTP transport now running");
        futures::select! {
            _ = servers.fuse() => (),
            _ = handles_dispatcher.fuse() => (),
            _ = self.handle_set.on_handles_dropped().fuse() => (),
        };
        info!("HTTP transport is done");

        Ok(())
    }
}

/// Handles a single request from a connection by sending it to the appropriate service.
async fn handle_request(
    request_tracker: Arc<RequestTracker>,
    services: Arc<Mutex<Services>>,
    clock: Clock,
    req: Request<Body>,
) -> Result<Response<Body>, RequestError> {
    let request_type = RequestType::from_url_path(req.uri().path())?;

    // Authentify the request using the authentication token and extract cell & node from it
    let auth_token_str = read_authorization_token(&req)?;
    let auth_token = AuthToken::decode_base58_string(&auth_token_str).map_err(|err| {
        warn!(
            "Unauthorized request for {:?} using token {}: {}",
            request_type, auth_token_str, err
        );
        RequestError::Unauthorized
    })?;

    let mut services = services.lock().await;
    let service = services
        .get_handle(auth_token.cell_id(), request_type.transport_layer())
        .ok_or_else(|| {
            warn!("Cell {} not found for request", auth_token.cell_id());
            RequestError::NotFound
        })?;

    let from_node = {
        let cell_nodes = service.cell.nodes();
        cell_nodes
            .get(auth_token.node_id())
            .map(|c| c.node().clone())
            .ok_or_else(|| {
                warn!(
                    "Node {} not found in cell {} for request",
                    auth_token.node_id(),
                    auth_token.cell_id()
                );
                RequestError::NotFound
            })?
    };

    if request_type == RequestType::EntitiesQuery {
        let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
        let tracked_request = request_tracker.push().await;
        let cell = service.cell.clone();

        send_entity_query(
            body_bytes.as_ref(),
            &clock,
            from_node,
            service,
            &tracked_request,
        )
        .await?;

        drop(services); // drop handles to release lock while we wait for answer

        Ok(receive_entity_query(&cell, tracked_request).await?)
    } else {
        Err(RequestError::NotFound)
    }
}

async fn send_entity_query(
    body_bytes: &[u8],
    clock: &Clock,
    from_node: Node,
    service: &mut ServiceChannels,
    tracked_request: &TrackedRequest,
) -> Result<(), RequestError> {
    let local_node = service.cell.local_node().node().clone();

    let mut frame_builder = CapnpFrameBuilder::<query_request::Owned>::new();
    let mut msg_builder = frame_builder.get_builder();
    msg_builder.set_request(body_bytes);

    let message =
        OutMessage::from_framed_message(&service.cell, TransportLayer::Index, frame_builder)?
            .with_to_node(local_node)
            .with_rendez_vous_id(clock.consistent_time(service.cell.local_node()))
            .with_connection(ConnectionID::HTTPServer(tracked_request.id()))
            .to_in_message(from_node)?;

    service.send_message(message)?;

    Ok(())
}

async fn receive_entity_query(
    cell: &Cell,
    tracked_request: TrackedRequest,
) -> Result<Response<Body>, RequestError> {
    let local_node = cell.local_node().node().clone();

    let response_message = tracked_request
        .get_response_or_timeout()
        .await
        .map_err(|_| RequestError::Server("Couldn't receive response from handle".to_string()))?;

    let message_envelope = response_message.envelope_builder.as_owned_frame();
    let message = InMessage::from_node_and_frame(local_node, message_envelope)?;
    let result_message = message.get_data_as_framed_message::<query_response::Owned>()?;
    let result_reader = result_message.get_reader()?;

    if !result_reader.has_error() {
        let body = Body::from(result_reader.get_response()?.to_vec());
        Ok(Response::new(body))
    } else {
        Err(RequestError::Query)
    }
}

fn read_authorization_token(request: &Request<Body>) -> Result<String, RequestError> {
    let pq = request.uri();
    let path_and_query = pq.path_and_query().ok_or(RequestError::Unauthorized)?;
    let query = path_and_query.query().ok_or(RequestError::Unauthorized)?;

    let params = url::form_urlencoded::parse(query.as_bytes());
    let token = get_query_token(params).ok_or(RequestError::Unauthorized)?;

    Ok(token.to_string())
}

fn get_query_token(pairs: url::form_urlencoded::Parse) -> Option<Cow<str>> {
    for (key, value) in pairs {
        if key == "token" {
            return Some(value);
        }
    }

    None
}

/// Type of an incoming HTTP request.
#[derive(Debug, PartialEq)]
enum RequestType {
    EntitiesQuery,
    EntitiesMutation,
}

impl RequestType {
    fn from_url_path(path: &str) -> Result<RequestType, RequestError> {
        if path == "/entities/query" {
            Ok(RequestType::EntitiesQuery)
        } else if path == "/entities/mutate" {
            Ok(RequestType::EntitiesMutation)
        } else {
            Err(RequestError::NotFound)
        }
    }

    fn transport_layer(&self) -> TransportLayer {
        match self {
            RequestType::EntitiesQuery => TransportLayer::Index,
            RequestType::EntitiesMutation => TransportLayer::Index,
        }
    }
}

/// Request related error.
#[derive(Debug, thiserror::Error)]
enum RequestError {
    #[error("Request not found")]
    NotFound,
    #[error("Request unauthorized")]
    Unauthorized,
    #[error("Query error")]
    Query,
    #[error("Internal server error: {0}")]
    Server(String),
    #[error("Transport error: {0}")]
    Transport(#[from] crate::Error),
    #[error("Capnp serialization error: {0}")]
    Serialization(#[from] capnp::Error),
    #[error("Hyper error: {0}")]
    Hyper(#[from] hyper::Error),
}

impl RequestError {
    fn to_response(&self) -> Response<Body> {
        let mut resp = Response::default();
        let status = match self {
            RequestError::NotFound => StatusCode::NOT_FOUND,
            RequestError::Unauthorized => StatusCode::UNAUTHORIZED,
            RequestError::Query => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        *resp.status_mut() = status;
        resp
    }
}

/// Services registered with the transport that can receive messages and reply to them.
#[derive(Default)]
struct Services {
    services: HashMap<(CellId, TransportLayer), ServiceChannels>,
}

impl Services {
    fn get_handle(
        &mut self,
        cell_id: &CellId,
        layer: TransportLayer,
    ) -> Option<&mut ServiceChannels> {
        self.services.get_mut(&(cell_id.clone(), layer))
    }

    fn remove_handle(&mut self, cell_id: &CellId, layer: TransportLayer) {
        self.services.remove(&(cell_id.clone(), layer));
    }
}

struct ServiceChannels {
    cell: Cell,
    in_sender: mpsc::Sender<InEvent>,
    out_receiver: Option<mpsc::Receiver<OutEvent>>,
}

impl ServiceChannels {
    fn send_message(&mut self, msg: Box<InMessage>) -> Result<(), RequestError> {
        self.in_sender
            .try_send(InEvent::Message(msg))
            .map_err(|err| RequestError::Server(format!("Couldn't send to handle: {}", err)))?;

        Ok(())
    }
}

/// Handle to the HTTP transport to be used by a service of a cell.
pub struct HTTPTransportServiceHandle {
    cell_id: CellId,
    layer: TransportLayer,
    inner: Weak<Mutex<Services>>,
    sink: Option<mpsc::Sender<OutEvent>>,
    stream: Option<mpsc::Receiver<InEvent>>,
    handle: Handle,
}

impl TransportHandle for HTTPTransportServiceHandle {
    type Sink = MpscHandleSink;
    type Stream = MpscHandleStream;

    fn on_started(&self) -> TransportHandleOnStart {
        Box::new(self.handle.on_set_started())
    }

    fn get_sink(&mut self) -> Self::Sink {
        MpscHandleSink::new(self.sink.take().expect("Sink was already consumed"))
    }

    fn get_stream(&mut self) -> Self::Stream {
        MpscHandleStream::new(self.stream.take().expect("Stream was already consumed"))
    }
}

impl Future for HTTPTransportServiceHandle {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.handle.on_set_dropped().poll_unpin(cx)
    }
}

impl Drop for HTTPTransportServiceHandle {
    fn drop(&mut self) {
        debug!(
            "Transport handle for cell {} layer {:?} got dropped. Removing it from transport",
            self.cell_id, self.layer
        );

        // we have been dropped, we remove ourself from layers to communicate with
        if let Some(inner) = self.inner.upgrade() {
            let mut inner = block_on(inner.lock());
            inner.remove_handle(&self.cell_id, self.layer);
        }
    }
}

#[cfg(test)]
mod tests {
    use exocore_core::{
        cell::{FullCell, LocalNode},
        futures::spawn_future,
        protos::generated::index_transport_capnp::query_response,
    };
    use hyper::{body::Buf, Client};

    use crate::testing::TestableTransportHandle;

    use super::*;

    #[tokio::test]
    async fn test_server() -> anyhow::Result<()> {
        exocore_core::logging::setup(None);

        let node = LocalNode::generate();
        let cell = FullCell::generate(node.clone());
        let clock = Clock::new();
        let layer = TransportLayer::Index;
        let auth_token = AuthToken::new(cell.cell(), &clock, None)?;

        let config = HTTPTransportConfig {
            listen_addresses: vec!["127.0.0.1:3007".parse()?],
            ..Default::default()
        };

        let mut server = HTTPTransportServer::new(config, clock);
        let handle = server.get_handle(cell.cell().clone(), layer)?;

        spawn_future(async move {
            server.run().await.unwrap();
        });

        handle.on_started().await;

        let mut index_handle = TestableTransportHandle::new(handle, cell.cell().clone());
        let http_client = Client::new();

        let url = format!(
            "http://127.0.0.1:3007/entities/query?token={}",
            auth_token.encode_base58_string()
        );
        let req = Request::builder()
            .method("POST")
            .uri(url)
            .body(Body::from("query body"))?;

        let (req_sender, req_recv) = futures::channel::oneshot::channel();
        spawn_future(async move {
            let resp = http_client.request(req).await;
            req_sender.send(resp).unwrap();
        });

        {
            let query_request = index_handle.recv_msg().await;
            let query_frame = query_request.get_data_as_framed_message::<query_request::Owned>()?;
            let query_reader = query_frame.get_reader()?;
            let query_body = query_reader.get_request()?;
            assert_eq!(query_body, b"query body");

            let mut frame_builder = CapnpFrameBuilder::<query_response::Owned>::new();
            let mut b: query_response::Builder = frame_builder.get_builder();
            b.set_response(b"response body");
            let resp_msg = query_request.to_response_message(cell.cell(), frame_builder)?;
            index_handle.send_message(resp_msg).await;
        }

        let response = req_recv.await??;
        let body = hyper::body::aggregate(response).await?;
        assert_eq!(body.bytes(), b"response body");

        Ok(())
    }
}
