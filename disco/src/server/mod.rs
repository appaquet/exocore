use crate::payload::{CreatePayloadRequest, CreatePayloadResponse, Payload, PayloadID};
use hyper::{
    service::{make_service_fn, service_fn},
    Body, Method, Request, Response, Server, StatusCode,
};

mod store;

pub struct DiscoServer {
    port: u16,
}

impl DiscoServer {
    pub fn new(port: u16) -> Self {
        Self { port }
    }

    pub async fn start(&self) -> anyhow::Result<()> {
        let store = store::Store::default();

        let addr = format!("0.0.0.0:{}", self.port).parse()?;
        let server = Server::bind(&addr).serve(make_service_fn(move |_socket| {
            let store = store.clone();

            async move {
                Ok::<_, hyper::Error>(service_fn(move |req| {
                    let store = store.clone();

                    async {
                        let resp = match Self::handle_request(req, store).await {
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

        server.await?;

        Ok(())
    }

    async fn handle_request(
        req: Request<Body>,
        store: store::Store,
    ) -> Result<Response<Body>, RequestError> {
        let request_type = RequestType::from_method_path(req.method(), req.uri().path())?;
        match request_type {
            RequestType::Post => Self::handle_post(req, store).await,
            RequestType::Get(id) => Self::handle_get(store, id).await,
            RequestType::Options => Self::handle_request_options().await,
        }
    }

    async fn handle_post(
        req: Request<Body>,
        store: store::Store,
    ) -> Result<Response<Body>, RequestError> {
        let req_body_bytes = hyper::body::to_bytes(req.into_body()).await?;
        let req_payload = serde_json::from_slice::<CreatePayloadRequest>(req_body_bytes.as_ref())
            .map_err(RequestError::Serialization)?;

        let (id, expiration) = store.push(req_payload.data).await;

        let resp_payload = CreatePayloadResponse { id, expiration };
        let resp_body_bytes =
            serde_json::to_vec(&resp_payload).map_err(RequestError::Serialization)?;
        let resp_body = Body::from(resp_body_bytes);

        Ok(Response::new(resp_body))
    }

    async fn handle_get(
        store: store::Store,
        id: PayloadID,
    ) -> Result<Response<Body>, RequestError> {
        let data = store.get(id).await.ok_or(RequestError::NotFound)?;

        let resp_payload = Payload { id, data };
        let resp_body_bytes =
            serde_json::to_vec(&resp_payload).map_err(RequestError::Serialization)?;
        let resp_body = Body::from(resp_body_bytes);

        Ok(Response::new(resp_body))
    }

    async fn handle_request_options() -> Result<Response<Body>, RequestError> {
        let mut resp = Response::default();

        let headers = resp.headers_mut();
        headers.insert(
            hyper::header::ACCESS_CONTROL_ALLOW_METHODS,
            "POST, GET".parse().unwrap(),
        );
        headers.insert(
            hyper::header::ACCESS_CONTROL_ALLOW_ORIGIN,
            "*".parse().unwrap(),
        );

        Ok(resp)
    }
}

#[derive(Debug, PartialEq)]
enum RequestType {
    Post,
    Get(PayloadID),
    Options,
}

impl RequestType {
    fn from_method_path(method: &Method, path: &str) -> Result<RequestType, RequestError> {
        match method {
            &Method::POST if path == "/" => Ok(RequestType::Post),
            &Method::GET => {
                let id: PayloadID = path
                    .replace("/", "")
                    .parse()
                    .map_err(|_| RequestError::InvalidRequestType)?;
                Ok(RequestType::Get(id))
            }
            &Method::OPTIONS => Ok(RequestType::Options),
            _ => Err(RequestError::InvalidRequestType),
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum RequestError {
    #[error("Invalid request type")]
    InvalidRequestType,
    #[error("Payload not found")]
    NotFound,
    #[error("Invalid request body: {0}")]
    Serialization(#[source] serde_json::Error),
    #[error("Hyper error: {0}")]
    Hyper(#[from] hyper::Error),
}

impl RequestError {
    fn to_response(&self) -> Response<Body> {
        let mut resp = Response::default();
        let status = match self {
            RequestError::InvalidRequestType => StatusCode::NOT_FOUND,
            RequestError::NotFound => StatusCode::NOT_FOUND,
            RequestError::Serialization(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };

        *resp.status_mut() = status;
        resp
    }
}
