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

    pub store: Option<LocalStore<DirectoryChainStore, MemoryPendingStore>>,
    store_handle: StoreHandle<DirectoryChainStore, MemoryPendingStore>,
    _temp_dir: TempDir,
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

        let store = LocalStore::new(schema.clone(), cluster.get_new_handle(0), index)?;
        let store_handle = store.get_handle()?;

        Ok(TestLocalStore {
            cluster,
            schema: schema.clone(),

            store: Some(store),
            store_handle,
            _temp_dir: temp_dir,
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

    pub fn query_via_handle(&mut self, query: Query) -> Result<QueryResult, failure::Error> {
        let resp_future = self.store_handle.query(query);
        self.cluster
            .runtime
            .block_on(resp_future)
            .map_err(|err| err.into())
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
}
