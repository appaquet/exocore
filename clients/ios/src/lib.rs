#![allow(clippy::not_unsafe_ptr_arg_deref)]

#[macro_use]
extern crate log;

mod logging;

use exocore_common::cell::Cell;
use exocore_common::crypto::keys::{Keypair, PublicKey};
use exocore_common::node::{LocalNode, Node};
use exocore_common::time::{Clock, ConsistentTimestamp};
use exocore_index::query::Query;
use exocore_index::store::remote::{Client, ClientConfiguration, ClientHandle};
use exocore_schema::schema::Schema;
use exocore_schema::serialization::with_schema;
use exocore_transport::lp2p::Libp2pTransportConfig;
use exocore_transport::{Libp2pTransport, TransportLayer};
use futures::prelude::*;
use libc;
use std::ffi::CString;
use std::sync::Arc;
use tokio::runtime::Runtime;

pub struct Context {
    runtime: Runtime,
    store_handle: ClientHandle,
    schema: Arc<Schema>,
}

impl Context {
    fn new() -> Result<Context, Status> {
        logging::setup(None);
        info!("Initializing...");

        let mut runtime = Runtime::new().expect("Couldn't start runtime");

        // TODO: To be cleaned up when cell management will be ironed out: https://github.com/appaquet/exocore/issues/80
        let local_node = LocalNode::new_from_keypair(Keypair::decode_base58_string("ae4WbDdfhv3416xs8S2tQgczBarmR8HKABvPCmRcNMujdVpDzuCJVQADVeqkqwvDmqYUUjLqv7kcChyCYn8R9BNgXP").unwrap());
        let local_addr = "/ip4/0.0.0.0/tcp/0"
            .parse()
            .expect("Couldn't parse local node");
        local_node.add_address(local_addr);

        let transport_config = Libp2pTransportConfig::default();
        let mut transport = Libp2pTransport::new(local_node.clone(), transport_config);

        let cell_pk =
            PublicKey::decode_base58_string("pe2AgPyBmJNztntK9n4vhLuEYN8P2kRfFXnaZFsiXqWacQ")
                .expect("Couldn't decode cell publickey");
        let cell = Cell::new(cell_pk, local_node.clone());
        let clock = Clock::new();
        let schema = exocore_schema::test_schema::create();

        let remote_node_pk =
            PublicKey::decode_base58_string("peFdPsQsdqzT2H6cPd3WdU1fGdATDmavh4C17VWWacZTMP")
                .expect("Couldn't decode cell publickey");
        let remote_node = Node::new_from_public_key(remote_node_pk);
        let remote_addr = "/ip4/192.168.2.13/tcp/3330"
            .parse()
            .expect("Couldn't parse remote node addr");
        remote_node.add_address(remote_addr);
        {
            cell.nodes_mut().add(remote_node.clone());
        }

        let store_transport = transport
            .get_handle(cell.clone(), TransportLayer::Index)
            .expect("Couldn't get transport handle for remote index");
        let remote_store_config = ClientConfiguration::default();
        let remote_store_client = Client::new(
            remote_store_config,
            cell,
            clock,
            schema.clone(),
            store_transport,
            remote_node,
        )
        .expect("Couldn't start remote store");

        let store_handle = remote_store_client
            .get_handle()
            .expect("Couldn't get store handle");

        runtime.spawn(
            transport
                .map(|_| {
                    error!("Transport is done");
                })
                .map_err(|err| {
                    error!("Error in transport: {}", err);
                }),
        );

        runtime.spawn(
            remote_store_client
                .map(|_| {
                    error!("Remote store is done");
                })
                .map_err(|err| {
                    error!("Error in remote store: {}", err);
                }),
        );

        Ok(Context {
            runtime,
            schema,
            store_handle,
        })
    }
}

#[repr(C)]
pub struct ExocoreContext {
    status: Status,
    context: *mut Context,
}

#[repr(u8)]
enum Status {
    Success = 0,
}

#[no_mangle]
pub extern "C" fn exocore_context_new() -> ExocoreContext {
    let context = match Context::new() {
        Ok(context) => context,
        Err(err) => {
            return ExocoreContext {
                status: err,
                context: std::ptr::null_mut(),
            }
        }
    };

    ExocoreContext {
        status: Status::Success,
        context: Box::into_raw(Box::new(context)),
    }
}

#[no_mangle]
pub extern "C" fn exocore_context_free(ctx: *mut Context) {
    unsafe {
        drop(Box::from_raw(ctx));
    }
}

#[no_mangle]
pub extern "C" fn exocore_query(
    ctx: *mut Context,
    query: *const libc::c_char,
    on_ready: extern "C" fn(status: QueryStatus, *const libc::c_char),
) -> QueryHandle {
    let context = unsafe { ctx.as_mut().unwrap() };

    let result_future = context
        .store_handle
        .query(Query::match_text("hello"))
        .expect("TODO"); // TODO:
    let query_id = result_future.query_id();

    let schema = context.schema.clone();
    context.runtime.spawn(
        result_future
            .then(move |res| {
                let ret = res.as_ref().map(|_| ()).map_err(|_| ());

                match res {
                    Ok(res) => {
                        let json = with_schema(&schema, || serde_json::to_string(&res)).unwrap();
                        info!("Got results: {:?}", json);

                        let cstr = CString::new(json).unwrap();
                        on_ready(QueryStatus::Success, cstr.as_ref().as_ptr());
                    }
                    Err(err) => println!("Got error"),
                }

                ret
            })
            .map(|_| ())
            .map_err(|_| ()),
    );

    QueryHandle {
        status: QueryStatus::Success,
        query_id: query_id.0,
    }
}

#[no_mangle]
pub extern "C" fn exocore_watched_query(
    ctx: *mut Context,
    query: *const libc::c_char,
    on_change: extern "C" fn(status: QueryStatus, *const libc::c_char),
) -> QueryHandle {
    let context = unsafe { ctx.as_mut().unwrap() };

    let result_stream = context
        .store_handle
        .watched_query(Query::match_text("hello"))
        .expect("TODO"); // TODO:
    let query_id = result_stream.query_id();

    let schema = context.schema.clone();
    context.runtime.spawn(
        result_stream
            .then(move |res| {
                let ret = res.as_ref().map(|_| ()).map_err(|_| ());

                match res {
                    Ok(res) => {
                        let json = with_schema(&schema, || serde_json::to_string(&res)).unwrap();
                        info!("Got results: {:?}", json);

                        let cstr = CString::new(json).unwrap();
                        on_change(QueryStatus::Success, cstr.as_ref().as_ptr());
                    }
                    Err(err) => println!("Got error"),
                }

                ret
            })
            .for_each(|_| Ok(()))
            .map_err(|_| ()),
    );

    QueryHandle {
        status: QueryStatus::Success,
        query_id: query_id.0,
    }
}

#[no_mangle]
pub extern "C" fn exocore_query_cancel(ctx: *mut Context, handle: QueryHandle) {
    let context = unsafe { ctx.as_mut().unwrap() };

    if let Err(err) = context
        .store_handle
        .cancel_query(ConsistentTimestamp(handle.query_id))
    {
        error!("Error cancelling query: {}", err)
    }
}

#[repr(u8)]
pub enum QueryStatus {
    Success = 0,
    Error,
}

#[repr(C)]
pub struct QueryHandle {
    status: QueryStatus,
    query_id: u64,
}
