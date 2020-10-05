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
    cell::{Cell, CellId, CellNodes},
    crypto::auth_token::AuthToken,
    framing::CapnpFrameBuilder,
    framing::FrameBuilder,
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
use requests::Requests;

// TODO: Try to merge libp2p handle tester
// TODO: Try to make mock use the exact same transport handle ?

pub struct HTTPTransportServer {
    config: HTTPTransportConfig,
    clock: Clock,
    handles: Arc<Mutex<Handles>>,
    handle_set: HandleSet,
}

impl HTTPTransportServer {
    pub fn new(config: HTTPTransportConfig, clock: Clock) -> HTTPTransportServer {
        HTTPTransportServer {
            config,
            clock,
            handles: Default::default(),
            handle_set: Default::default(),
        }
    }

    pub fn get_handle(
        &mut self,
        cell: Cell,
        layer: TransportLayer,
    ) -> Result<HTTPTransportHandle, Error> {
        let (in_sender, in_receiver) = mpsc::channel(self.config.handle_in_channel_size);
        let (out_sender, out_receiver) = mpsc::channel(self.config.handle_out_channel_size);

        // Register new handle and its streams
        let mut handles = block_on(self.handles.lock());
        let inner_layer = HandleChannels {
            cell: cell.clone(),
            in_sender,
            out_receiver: Some(out_receiver),
        };
        info!(
            "Registering transport for cell {} and layer {:?}",
            cell, layer
        );
        let key = (cell.id().clone(), layer);
        handles.handles.insert(key, inner_layer);

        Ok(HTTPTransportHandle {
            cell_id: cell.id().clone(),
            layer,
            inner: Arc::downgrade(&self.handles),
            sink: Some(out_sender),
            stream: Some(in_receiver),
            handle: self.handle_set.get_handle(),
        })
    }

    pub async fn run(self) -> Result<(), Error> {
        let connections = Arc::new(Requests::new(self.config));

        // TODO: Listen to all addresses
        let addr = ([127, 0, 0, 1], 3007).into();
        let server = {
            let connections = connections.clone();
            let handles = self.handles.clone();
            let clock = self.clock.clone();
            Server::bind(&addr).serve(make_service_fn(move |_socket| {
                let connections = connections.clone();
                let handles = handles.clone();
                let clock = clock.clone();
                async move {
                    Ok::<_, hyper::Error>(service_fn(move |req| {
                        let connections = connections.clone();
                        let handles = handles.clone();
                        let clock = clock.clone();

                        async {
                            let resp = Self::handle_request(connections, handles, clock, req).await;

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
            }))
        };

        // Takes care of outgoing messages from handles to be dispatched to connections
        let handles_dispatcher = {
            let handles = self.handles.clone();
            let connections = connections.clone();

            async move {
                let mut inner = handles.lock().await;

                let mut futures = Vec::new();
                for handle_channels in inner.handles.values_mut() {
                    let mut out_receiver = handle_channels
                        .out_receiver
                        .take()
                        .expect("Out receiver of one layer was already consumed");

                    let connections = connections.clone();
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
            _ = server.fuse() => (),
            _ = handles_dispatcher.fuse() => (),
            _ = self.handle_set.on_handles_dropped().fuse() => (),
        };
        info!("HTTP transport is done");

        Ok(())
    }

    async fn handle_request(
        connections: Arc<Requests>,
        handles: Arc<Mutex<Handles>>,
        clock: Clock,
        req: Request<Body>,
    ) -> Result<Response<Body>, RequestError> {
        let req_path = req.uri().path().to_string();

        let token_str = read_authorization_token(&req)?;
        let token = AuthToken::decode_base58_string(&token_str).map_err(|err| {
            warn!(
                "Unauthorized request to {} using token {}: {}",
                req_path, token_str, err
            );
            RequestError::Unauthorized
        })?;

        let body_bytes = hyper::body::to_bytes(req.into_body()).await?;
        info!("Request {}", req_path);

        // TODO: Drop handles once we sent request to allow other requests
        let mut handles = handles.lock().await;

        if req_path == "/entities/query" {
            let handle = handles
                .get_handle(token.cell_id(), TransportLayer::Index)
                .ok_or_else(|| {
                    warn!("Cell {} not found for entities request", token.cell_id());
                    RequestError::NotFound
                })?;

            let from_node = {
                let cell_nodes = handle.cell.nodes();
                cell_nodes.get(token.node_id()).map(|c| c.node().clone())
            }
            .ok_or_else(|| {
                warn!(
                    "Node {} not found in cell {} for entities request",
                    token.node_id(),
                    token.cell_id()
                );
                RequestError::NotFound
            })?;

            let response_channel = connections.push().await;

            let cell = &handle.cell;
            let request_id = clock.consistent_time(cell.local_node());
            let local_node = cell.local_node().node().clone();

            let event = {
                let mut frame_builder = CapnpFrameBuilder::<query_request::Owned>::new();
                let mut msg_builder = frame_builder.get_builder();
                msg_builder.set_request(body_bytes.as_ref());

                let msg = OutMessage::from_framed_message(
                    &handle.cell,
                    TransportLayer::Index,
                    frame_builder,
                )?
                .with_to_node(local_node.clone())
                .with_rendez_vous_id(request_id)
                .with_connection(ConnectionID::HTTPServer(response_channel.id()))
                .to_in_message(from_node)?;

                InEvent::Message(msg)
            };
            handle
                .in_sender
                .try_send(event)
                .map_err(|err| RequestError::Server(format!("Couldn't send to handle: {}", err)))?;

            // drop handles to release lock while we wait for answer
            drop(handles);

            let response = response_channel
                .get_response_or_timeout()
                .await
                .map_err(|_| {
                    RequestError::Server("Couldn't receive response from handle".to_string())
                })?;

            let envelope = response.envelope_builder.as_owned_frame();
            let in_message = InMessage::from_node_and_frame(local_node.clone(), envelope)?;
            let query_result = in_message.get_data_as_framed_message::<query_response::Owned>()?;
            let query_result_reader = query_result.get_reader()?;

            // TODO: Check if query result is an error
            let body = Body::from(query_result_reader.get_response()?.to_vec());

            Ok(Response::new(body))
        } else {
            Err(RequestError::NotFound)
        }
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

#[derive(Debug, thiserror::Error)]
enum RequestError {
    #[error("Request not found")]
    NotFound,
    #[error("Request unauthorized")]
    Unauthorized,
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
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        *resp.status_mut() = status;
        resp
    }
}

#[derive(Default)]
struct Handles {
    handles: HashMap<(CellId, TransportLayer), HandleChannels>,
}

impl Handles {
    fn get_handle(
        &mut self,
        cell_id: &CellId,
        layer: TransportLayer,
    ) -> Option<&mut HandleChannels> {
        self.handles.get_mut(&(cell_id.clone(), layer))
    }

    fn remove_handle(&mut self, cell_id: &CellId, layer: TransportLayer) {
        self.handles.remove(&(cell_id.clone(), layer));
    }
}

struct HandleChannels {
    cell: Cell,
    in_sender: mpsc::Sender<InEvent>,
    out_receiver: Option<mpsc::Receiver<OutEvent>>,
}

pub struct HTTPTransportHandle {
    cell_id: CellId,
    layer: TransportLayer,
    inner: Weak<Mutex<Handles>>,
    sink: Option<mpsc::Sender<OutEvent>>,
    stream: Option<mpsc::Receiver<InEvent>>,
    handle: Handle,
}

impl TransportHandle for HTTPTransportHandle {
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

impl Future for HTTPTransportHandle {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        self.handle.on_set_dropped().poll_unpin(cx)
    }
}

impl Drop for HTTPTransportHandle {
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
