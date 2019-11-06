use std::sync::Arc;

use failure::err_msg;
use futures::{Future, Sink, Stream};
use tempdir::TempDir;

use exocore_common::node::LocalNode;
use exocore_data::tests_utils::DataTestCluster;
use exocore_data::{DirectoryChainStore, MemoryPendingStore};
use exocore_schema::entity::{EntityId, RecordBuilder, TraitBuilder, TraitId};
use exocore_schema::schema::Schema;
use exocore_transport::mock::MockTransportHandle;
use exocore_transport::transport::{MpscHandleSink, MpscHandleStream};
use exocore_transport::{
    InEvent, InMessage, OutEvent, OutMessage, TransportHandle, TransportLayer,
};

use crate::mutation::{Mutation, MutationResult, PutTraitMutation};
use crate::query::{Query, QueryResult};
use crate::store::local::store::StoreHandle;
use crate::store::local::traits_index::TraitsIndexConfig;
use crate::store::local::EntitiesIndexConfig;
use crate::store::AsyncStore;

use super::*;

///
/// Utility to test local store
///
pub struct TestLocalStore {
    pub cluster: DataTestCluster,
    pub schema: Arc<Schema>,

    store: Option<LocalStore<DirectoryChainStore, MemoryPendingStore, MockTransportHandle>>,
    store_handle: StoreHandle<DirectoryChainStore, MemoryPendingStore>,
    _temp_dir: TempDir,

    // external node & transport used to communicate with store
    external_node: LocalNode,
    external_transport_sink: Option<MpscHandleSink>,
    external_transport_stream: Option<MpscHandleStream>,
}

impl TestLocalStore {
    pub fn new() -> Result<TestLocalStore, failure::Error> {
        let mut cluster = DataTestCluster::new_single_and_start()?;

        let temp_dir = tempdir::TempDir::new("store")?;
        let schema = exocore_schema::test_schema::create();

        let index_config = EntitiesIndexConfig {
            pending_index_config: TraitsIndexConfig {
                indexer_num_threads: Some(1),
                ..TraitsIndexConfig::default()
            },
            chain_index_config: TraitsIndexConfig {
                indexer_num_threads: Some(1),
                ..TraitsIndexConfig::default()
            },
            ..EntitiesIndexConfig::default()
        };
        let index = EntitiesIndex::<DirectoryChainStore, MemoryPendingStore>::open_or_create(
            temp_dir.path(),
            index_config,
            schema.clone(),
            cluster.get_handle(0).try_clone()?,
        )?;

        let transport = cluster
            .transport_hub
            .get_transport(cluster.nodes[0].clone(), TransportLayer::Index);

        let store = LocalStore::new(
            cluster.cells[0].clone(),
            schema.clone(),
            cluster.get_new_handle(0),
            index,
            transport,
        )?;
        let store_handle = store.get_handle()?;

        // external node & transport used to communicate with store
        let external_node = LocalNode::generate();
        let mut external_transport_handle = cluster
            .transport_hub
            .get_transport(external_node.clone(), TransportLayer::Index);
        let external_transport_sink = external_transport_handle.get_sink();
        let external_transport_stream = external_transport_handle.get_stream();
        cluster.runtime.spawn(
            external_transport_handle
                .map(|_| {
                    info!("Transport handle completed");
                })
                .map_err(|err| {
                    error!("Transport handle error: {}", err);
                }),
        );

        Ok(TestLocalStore {
            cluster,
            schema: schema.clone(),

            store: Some(store),
            store_handle,
            _temp_dir: temp_dir,

            external_node,
            external_transport_sink: Some(external_transport_sink),
            external_transport_stream: Some(external_transport_stream),
        })
    }

    pub fn start_store(&mut self) -> Result<(), failure::Error> {
        let store = self.store.take().unwrap();
        self.cluster.runtime.spawn(
            store
                .map(|_| {
                    info!("Test store completed");
                })
                .map_err(|err| {
                    error!("Test store future failed: {}", err);
                }),
        );
        self.cluster
            .runtime
            .block_on(self.store_handle.on_start()?)?;
        Ok(())
    }

    pub fn mutate_via_handle(
        &mut self,
        mutation: Mutation,
    ) -> Result<MutationResult, failure::Error> {
        let resp_future = self.store_handle.mutate(mutation);
        self.cluster
            .runtime
            .block_on(resp_future)
            .map_err(|err| err.into())
    }

    pub fn mutate_via_transport(
        &mut self,
        mutation: Mutation,
    ) -> Result<MutationResult, failure::Error> {
        let mutation_frame = mutation.to_mutation_request_frame(&self.schema)?;

        // send message to store
        let rendez_vous_id = self.cluster.clocks[0].consistent_time(&self.cluster.nodes[0]);
        let external_cell = self.cluster.cells[0].clone_for_local_node(self.external_node.clone());
        let out_message =
            OutMessage::from_framed_message(&external_cell, TransportLayer::Index, mutation_frame)?
                .with_to_node(self.cluster.nodes[0].node().clone())
                .with_rendez_vous_id(rendez_vous_id);

        let sink = self.cluster.runtime.block_on(
            self.external_transport_sink
                .take()
                .unwrap()
                .send(OutEvent::Message(out_message)),
        )?;
        self.external_transport_sink = Some(sink);

        // wait for response from store
        let msg = self.wait_receive_next_message()?;

        // read response into mutation response
        let resp_frame = msg.get_data_as_framed_message()?;
        let response = MutationResult::from_response_frame(&self.schema, resp_frame)?;
        assert_eq!(msg.rendez_vous_id, Some(rendez_vous_id));

        Ok(response)
    }

    pub fn query_via_handle(&mut self, query: Query) -> Result<QueryResult, failure::Error> {
        let resp_future = self.store_handle.query(query);
        self.cluster
            .runtime
            .block_on(resp_future)
            .map_err(|err| err.into())
    }

    pub fn query_via_transport(&mut self, query: Query) -> Result<QueryResult, failure::Error> {
        let query_frame = query.to_query_request_frame(&self.schema)?;

        // send message to store
        let rendez_vous_id = self.cluster.clocks[0].consistent_time(&self.cluster.nodes[0]);
        let external_cell = self.cluster.cells[0].clone_for_local_node(self.external_node.clone());
        let out_message =
            OutMessage::from_framed_message(&external_cell, TransportLayer::Index, query_frame)?
                .with_to_node(self.cluster.nodes[0].node().clone())
                .with_rendez_vous_id(rendez_vous_id);

        let sink = self.cluster.runtime.block_on(
            self.external_transport_sink
                .take()
                .unwrap()
                .send(OutEvent::Message(out_message)),
        )?;
        self.external_transport_sink = Some(sink);

        // wait for response from store
        let in_msg = self.wait_receive_next_message()?;

        // read response into a results
        let resp_frame = in_msg.get_data_as_framed_message()?;
        let results = QueryResult::from_query_frame(&self.schema, resp_frame)?;

        assert_eq!(in_msg.rendez_vous_id, Some(rendez_vous_id));

        Ok(results)
    }

    pub fn create_put_contact_mutation<E: Into<EntityId>, T: Into<TraitId>, N: Into<String>>(
        &self,
        entity_id: E,
        trait_id: T,
        name: N,
    ) -> Mutation {
        Mutation::PutTrait(PutTraitMutation {
            entity_id: entity_id.into(),
            trt: TraitBuilder::new(&self.schema, "exocore", "contact")
                .unwrap()
                .set("id", trait_id.into())
                .set("name", name.into())
                .build()
                .unwrap(),
        })
    }

    fn wait_receive_next_message(&mut self) -> Result<Box<InMessage>, failure::Error> {
        loop {
            let (received, stream) = self.cluster.runtime.block_on(
                self.external_transport_stream
                    .take()
                    .unwrap()
                    .into_future()
                    .map_err(|(err, _stream)| {
                        err_msg(format!("Error receiving from stream: {}", err))
                    }),
            )?;
            self.external_transport_stream = Some(stream);

            match received {
                Some(InEvent::Message(msg)) => {
                    return Ok(msg);
                }
                _ => {
                    continue;
                }
            }
        }
    }
}
