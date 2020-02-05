#![allow(clippy::not_unsafe_ptr_arg_deref)]

#[macro_use]
extern crate log;

use std::os::raw::c_void;
use std::sync::{Arc, Once};

use futures::compat::Future01CompatExt;
use futures::StreamExt;
use libc;
use prost::Message;

use exocore_common::cell::Cell;
use exocore_common::crypto::keys::{Keypair, PublicKey};
use exocore_common::futures::Runtime;
use exocore_common::node::{LocalNode, Node};
use exocore_common::protos::generated::exocore_index::{EntityMutation, EntityQuery};
use exocore_common::protos::prost::ProstMessageExt;
use exocore_common::time::{Clock, ConsistentTimestamp};
use exocore_index::store::remote::{Client, ClientConfiguration, ClientHandle};
use exocore_transport::lp2p::Libp2pTransportConfig;
use exocore_transport::{Libp2pTransport, TransportHandle, TransportLayer};

mod context;
mod logging;

static INIT: Once = Once::new();

pub struct Context {
    runtime: Runtime,
    store_handle: Arc<ClientHandle>,
}

impl Context {
    fn new() -> Result<Context, ContextStatus> {
        INIT.call_once(|| {
            logging::setup(Some(log::LevelFilter::Debug));
        });

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
        let cell = Cell::new(cell_pk, local_node);
        let clock = Clock::new();

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
            cell.clone(),
            clock,
            store_transport,
            remote_node,
        )
        .map_err(|err| {
            error!("Couldn't create remote store client: {}", err);
            ContextStatus::Error
        })?;

        let store_handle = Arc::new(remote_store_client.get_handle());
        let management_transport_handle = transport
            .get_handle(cell, TransportLayer::None)
            .map_err(|err| {
                error!("Couldn't get transport handle: {}", err);
                ContextStatus::Error
            })?;

        runtime.spawn_std(async move {
            let res = transport.run().await;
            info!("Transport is done: {:?}", res);
        });

        runtime.block_on_std(management_transport_handle.on_started());

        runtime.spawn_std(async move {
            let _ = remote_store_client.run().await;
            info!("Remote store is done");
        });

        Ok(Context {
            runtime,
            store_handle,
        })
    }

    pub fn mutate(
        &mut self,
        mutation_bytes: *const libc::c_uchar,
        mutation_size: usize,
        callback: extern "C" fn(status: MutationStatus, *const libc::c_uchar, usize, *const c_void),
        callback_ctx: *const c_void,
    ) -> Result<MutationHandle, MutationStatus> {
        let mutation_bytes = unsafe { std::slice::from_raw_parts(mutation_bytes, mutation_size) };
        let mutation = EntityMutation::decode(mutation_bytes).map_err(|_| MutationStatus::Error)?;

        let store_handle = self.store_handle.clone();

        let callback_ctx = CallbackContext { ctx: callback_ctx };
        self.runtime.spawn_std(async move {
            let future_result = store_handle.mutate(mutation);

            let result = future_result.await;
            match result {
                Ok(res) => {
                    let encoded = match res.encode_to_vec() {
                        Ok(res) => res,
                        Err(err) => {
                            error!("Error decoding mutation result: {}", err);
                            callback(MutationStatus::Error, std::ptr::null(), 0, callback_ctx.ctx);
                            return;
                        }
                    };

                    debug!("Mutation result received");
                    callback(
                        MutationStatus::Success,
                        encoded.as_ptr(),
                        encoded.len(),
                        callback_ctx.ctx,
                    );
                }

                Err(err) => {
                    warn!("Mutation future has failed: {}", err);
                    callback(MutationStatus::Error, std::ptr::null(), 0, callback_ctx.ctx);
                }
            }
        });

        Ok(MutationHandle {
            status: MutationStatus::Success,
        })
    }

    pub fn query(
        &mut self,
        query_bytes: *const libc::c_uchar,
        query_size: usize,
        callback: extern "C" fn(status: QueryStatus, *const libc::c_uchar, usize, *const c_void),
        callback_ctx: *const c_void,
    ) -> Result<QueryHandle, QueryStatus> {
        let query_bytes = unsafe { std::slice::from_raw_parts(query_bytes, query_size) };
        let query = EntityQuery::decode(query_bytes).map_err(|_| QueryStatus::Error)?;

        let future_result = self.store_handle.query(query);
        let query_id = future_result.query_id();

        let callback_ctx = CallbackContext { ctx: callback_ctx };
        self.runtime.spawn_std(async move {
            let result = future_result.await;
            match result {
                Ok(res) => {
                    let encoded = match res.encode_to_vec() {
                        Ok(res) => res,
                        Err(err) => {
                            error!("Error decoding query result: {}", err);
                            callback(QueryStatus::Error, std::ptr::null(), 0, callback_ctx.ctx);
                            return;
                        }
                    };

                    debug!("Query results received");
                    callback(
                        QueryStatus::Success,
                        encoded.as_ptr(),
                        encoded.len(),
                        callback_ctx.ctx,
                    );
                }

                Err(err) => {
                    warn!("Query future has failed: {}", err);
                    callback(QueryStatus::Error, std::ptr::null(), 0, callback_ctx.ctx);
                }
            }
        });

        Ok(QueryHandle {
            status: QueryStatus::Success,
            query_id: query_id.0,
        })
    }

    pub fn watched_query(
        &mut self,
        query_bytes: *const libc::c_uchar,
        query_size: usize,
        callback: extern "C" fn(status: QueryStatus, *const libc::c_uchar, usize, *const c_void),
        callback_ctx: *const c_void,
    ) -> Result<QueryStreamHandle, QueryStreamStatus> {
        let query_bytes = unsafe { std::slice::from_raw_parts(query_bytes, query_size) };
        let query = EntityQuery::decode(query_bytes).map_err(|_| QueryStreamStatus::Error)?;

        let result_stream = self.store_handle.watched_query(query);
        let query_id = result_stream.query_id();

        let callback_ctx = CallbackContext { ctx: callback_ctx };
        self.runtime.spawn_std(async move {
            let mut stream = result_stream;

            while let Some(result) = stream.next().await {
                match result {
                    Ok(res) => {
                        let encoded = match res.encode_to_vec() {
                            Ok(res) => res,
                            Err(err) => {
                                error!("Error decoding watched query result: {}", err);
                                callback(QueryStatus::Error, std::ptr::null(), 0, callback_ctx.ctx);
                                return;
                            }
                        };

                        debug!("Watched query results received");
                        callback(
                            QueryStatus::Success,
                            encoded.as_ptr(),
                            encoded.len(),
                            callback_ctx.ctx,
                        );
                    }

                    Err(err) => {
                        warn!("Watched query has failed: {}", err);
                        callback(QueryStatus::Error, std::ptr::null(), 0, callback_ctx.ctx);
                        return;
                    }
                }
            }

            info!("Watched query done");
            callback(QueryStatus::Done, std::ptr::null(), 0, callback_ctx.ctx);
        });

        Ok(QueryStreamHandle {
            status: QueryStreamStatus::Success,
            query_id: query_id.0,
        })
    }
}

#[repr(C)]
pub struct ContextResult {
    status: ContextStatus,
    context: *mut Context,
}

#[repr(u8)]
enum ContextStatus {
    Success = 0,
    Error,
}

struct CallbackContext {
    ctx: *const c_void,
}

unsafe impl Send for CallbackContext {}

unsafe impl Sync for CallbackContext {}

#[repr(u8)]
pub enum QueryStatus {
    Success = 0,
    Done = 1,
    Error,
}

#[repr(C)]
pub struct QueryHandle {
    status: QueryStatus,
    query_id: u64,
}

#[repr(u8)]
pub enum QueryStreamStatus {
    Success = 0,
    Done,
    Error,
}

#[repr(C)]
pub struct QueryStreamHandle {
    status: QueryStreamStatus,
    query_id: u64,
}

#[repr(u8)]
pub enum MutationStatus {
    Success = 0,
    Error,
}

#[repr(C)]
pub struct MutationHandle {
    status: MutationStatus,
}

#[no_mangle]
pub extern "C" fn exocore_context_new() -> ContextResult {
    let context = match Context::new() {
        Ok(context) => context,
        Err(err) => {
            return ContextResult {
                status: err,
                context: std::ptr::null_mut(),
            };
        }
    };

    ContextResult {
        status: ContextStatus::Success,
        context: Box::into_raw(Box::new(context)),
    }
}

#[no_mangle]
pub extern "C" fn exocore_context_free(ctx: *mut Context) {
    let context = unsafe { Box::from_raw(ctx) };

    let Context {
        runtime,
        store_handle,
        ..
    } = *context;

    info!("Dropping handle...");

    // dropping store will cancel all queries' future
    drop(store_handle);

    info!("Waiting for runtime to be done");

    // wait for all queries future to be completed
    if futures::executor::block_on(runtime.shutdown_on_idle().compat()).is_err() {
        error!("Error shutting down runtime");
    }
}

#[no_mangle]
pub extern "C" fn exocore_mutation(
    ctx: *mut Context,
    mutation_bytes: *const libc::c_uchar,
    mutation_size: usize,
    callback: extern "C" fn(status: MutationStatus, *const libc::c_uchar, usize, *const c_void),
    callback_ctx: *const c_void,
) -> MutationHandle {
    let context = unsafe { ctx.as_mut().unwrap() };

    match context.mutate(mutation_bytes, mutation_size, callback, callback_ctx) {
        Ok(res) => res,
        Err(status) => MutationHandle { status },
    }
}

#[no_mangle]
pub extern "C" fn exocore_query(
    ctx: *mut Context,
    query_bytes: *const libc::c_uchar,
    query_size: usize,
    callback: extern "C" fn(status: QueryStatus, *const libc::c_uchar, usize, *const c_void),
    callback_ctx: *const c_void,
) -> QueryHandle {
    let context = unsafe { ctx.as_mut().unwrap() };

    match context.query(query_bytes, query_size, callback, callback_ctx) {
        Ok(res) => res,
        Err(status) => QueryHandle {
            status,
            query_id: 0,
        },
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

#[no_mangle]
pub extern "C" fn exocore_watched_query(
    ctx: *mut Context,
    query_bytes: *const libc::c_uchar,
    query_size: usize,
    callback: extern "C" fn(status: QueryStatus, *const libc::c_uchar, usize, *const c_void),
    callback_ctx: *const c_void,
) -> QueryStreamHandle {
    let context = unsafe { ctx.as_mut().unwrap() };

    match context.watched_query(query_bytes, query_size, callback, callback_ctx) {
        Ok(res) => res,
        Err(status) => QueryStreamHandle {
            status,
            query_id: 0,
        },
    }
}

#[no_mangle]
pub extern "C" fn exocore_watched_query_cancel(ctx: *mut Context, handle: QueryStreamHandle) {
    let context = unsafe { ctx.as_mut().unwrap() };

    if let Err(err) = context
        .store_handle
        .cancel_query(ConsistentTimestamp(handle.query_id))
    {
        error!("Error cancelling query stream: {}", err)
    }
}
