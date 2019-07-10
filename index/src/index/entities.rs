use super::traits::TraitsIndex;
use crate::domain::entity::Entity;
use crate::domain::schema;
use crate::domain::schema::Schema;
use crate::error::Error;
use crate::index::traits::{IndexMutation, PutTraitMutation, TraitsIndexConfig};
use crate::mutation::Mutation;
use crate::query::*;
use crate::results::{EntitiesResults, EntityResult, EntityResultSource};
use exocore_data::block::{BlockDepth, BlockOffset};
use exocore_data::engine::{EngineOperation, Event};
use exocore_data::operation::{Operation, OperationId};
use exocore_data::{chain, pending};
use exocore_data::{EngineHandle, EngineOperationStatus};
use std::path::{Path, PathBuf};
use std::sync::Arc;

///
///
///
#[derive(Clone, Copy, Debug)]
pub struct EntitiesIndexConfig {
    /// When should we index a block in the chain so that odds that we aren't going to revert it are high enough.
    /// Related to `CommitManagerConfig`.`operations_cleanup_after_block_depth`
    pub chain_index_min_depth: BlockDepth,

    /// Configuration for the in-memory traits index that are in the pending store
    pub pending_index_config: TraitsIndexConfig,

    /// Configuration for the persisted traits index that are in the chain
    pub chain_index_config: TraitsIndexConfig,
}

impl Default for EntitiesIndexConfig {
    fn default() -> Self {
        EntitiesIndexConfig {
            chain_index_min_depth: 3,
            pending_index_config: TraitsIndexConfig::default(),
            chain_index_config: TraitsIndexConfig::default(),
        }
    }
}

///
///
///
pub struct EntitiesIndex<CS, PS>
where
    CS: chain::ChainStore,
    PS: pending::PendingStore,
{
    config: EntitiesIndexConfig,
    pending_index: TraitsIndex,
    chain_index_dir: PathBuf,
    chain_index: TraitsIndex,

    schema: Arc<Schema>,

    // TODO: status => mark as incomplete if status not fully indexed
    phantom: std::marker::PhantomData<(CS, PS)>,
}

impl<CS, PS> EntitiesIndex<CS, PS>
where
    CS: chain::ChainStore,
    PS: pending::PendingStore,
{
    pub fn open_or_create(
        data_dir: &Path,
        config: EntitiesIndexConfig,
        schema: Arc<schema::Schema>,
    ) -> Result<EntitiesIndex<CS, PS>, Error> {
        let pending_index =
            TraitsIndex::create_in_memory(config.pending_index_config, schema.clone())?;

        // make sure directories are created
        let mut chain_index_dir = data_dir.to_path_buf();
        chain_index_dir.push("chain");
        if std::fs::metadata(&chain_index_dir).is_err() {
            std::fs::create_dir_all(&chain_index_dir)?;
        }

        let chain_index = TraitsIndex::open_or_create_mmap(
            config.chain_index_config,
            schema.clone(),
            &chain_index_dir,
        )?;

        Ok(EntitiesIndex {
            config,
            pending_index,
            chain_index_dir,
            chain_index,
            schema,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn write_mutation(
        &mut self,
        data_handle: &EngineHandle<CS, PS>,
        mutation: Mutation,
    ) -> Result<OperationId, Error> {
        let json_mutation = mutation.to_json(self.schema.clone())?;
        let op_id = data_handle.write_entry_operation(json_mutation.as_bytes())?;
        Ok(op_id)
    }

    pub fn handle_engine_event(
        &mut self,
        data_handle: &EngineHandle<CS, PS>,
        event: Event,
    ) -> Result<(), Error> {
        match event {
            Event::Started => {
                // TODO: status = unknown
                info!("Data engine is ready, indexing pending store & chain");
                self.index_chain_new_blocks(data_handle)?;
                self.reindex_pending(data_handle)?;
            }
            Event::StreamDiscontinuity => {
                // TODO: status = unknown
                warn!("Got a stream discontinuity. Forcing re-indexation of pending...");
                self.reindex_pending(data_handle)?;
            }
            Event::PendingOperationNew(op_id) => {
                self.handle_pending_operation_new(data_handle, op_id)?;
            }
            Event::ChainBlockNew(block_offset) => {
                debug!(
                    "Got new block at offset {}, checking if we can index a new block",
                    block_offset
                );
                self.index_chain_new_blocks(data_handle)?;
            }
            Event::ChainDiverged(diverged_block_offset) => {
                let highest_indexed_block = self.chain_index.highest_indexed_block()?;
                warn!(
                    "Chain has diverged at offset={}. Highest indexed block at = {:?}",
                    diverged_block_offset, highest_indexed_block
                );

                if let Some(last_indexed_offset) = highest_indexed_block {
                    if last_indexed_offset < diverged_block_offset {
                        // since we only index blocks that have a certain depth, and therefor higher
                        // probability of being definitive, if we have a divergence, we can just re-index
                        // the pending store which should still contain operations that are in our invalid
                        // chain
                        warn!("Divergence is after last indexed offset, we only re-index pending");
                        self.reindex_pending(data_handle)?;
                    } else {
                        // if we are here, we indexed a block from the chain that isn't valid anymore
                        // since we are deleting traits that got deleted from the actual index, there is no
                        // way to rollback to the diverged offset, and will require a re-index
                        return Err(Error::Fatal(
                            format!("Chain has diverged at an offset={}, which is before last indexed block at offset {}",
                                                        diverged_block_offset,last_indexed_offset
                        )));
                    }
                } else {
                    warn!("Diverged with an empty chain index. Re-indexing...");
                    self.reindex_chain(data_handle)?;
                }
            }
        }

        Ok(())
    }

    pub fn search(
        &self,
        data_handle: &EngineHandle<CS, PS>,
        query: &Query,
    ) -> Result<EntitiesResults, Error> {
        // TODO: fetch all traits for each entity
        // TODO: check if we have any deletion
        // TODO: limit

        let chain_results = self
            .chain_index
            .search(query, 50)?
            .into_iter()
            .map(|res| (res, EntityResultSource::Chain));
        let pending_results = self
            .pending_index
            .search(query, 50)?
            .into_iter()
            .map(|res| (res, EntityResultSource::Pending));
        debug!(
            "Found {} from chain, {} from pending",
            chain_results.len(),
            pending_results.len()
        );

        // TODO: need to properly merge, and use thombstone
        // TODO: need to exclude all pending that were committed, but didn't get delete yet
        let results = chain_results
            .into_iter()
            .chain(pending_results.into_iter())
            .map(|(trait_res, source)| EntityResult {
                entity: Entity::new(trait_res.entity_id),
                source,
            })
            .collect::<Vec<_>>();
        let total_estimated = results.len() as u32;

        Ok(EntitiesResults {
            results,
            next_page: None,
            current_page: QueryPaging {
                count: 0,
                from_token: None,
                to_token: None,
            },
            total_estimated,
        })
    }

    fn reindex_pending(&mut self, data_handle: &EngineHandle<CS, PS>) -> Result<(), Error> {
        info!("Clearing & reindexing pending index");

        self.pending_index =
            TraitsIndex::create_in_memory(self.config.pending_index_config, self.schema.clone())?;

        // create an iterator over operations from chain (if any) and pending store
        let pending_iter = data_handle.get_pending_operations(..)?.into_iter();
        let pending_and_chain_iter: Box<dyn Iterator<Item = EngineOperation>> =
            if let Some((last_indexed_offset, _last_indexed_depth)) =
                self.last_indexed_block(data_handle)?
            {
                // filter pending to not include operations that are now in the chain
                let pending_iter =
                    pending_iter.filter(move |op| op.status == EngineOperationStatus::Pending);

                // take operations from chain that have not been indexed to the chain index yet
                let chain_iter = data_handle
                    .get_chain_operations(Some(last_indexed_offset))
                    .filter(move |op| {
                        if let EngineOperationStatus::Committed(offset, _depth) = op.status {
                            offset > last_indexed_offset
                        } else {
                            false
                        }
                    });

                Box::new(chain_iter.chain(pending_iter))
            } else {
                Box::new(pending_iter)
            };

        let schema = self.schema.clone();
        let mutations_iter = pending_and_chain_iter
            .flat_map(|op| Self::pending_operation_to_index_mutation(schema.clone(), op));
        self.pending_index.apply_mutations(mutations_iter)?;

        Ok(())
    }

    fn reindex_chain(&mut self, data_handle: &EngineHandle<CS, PS>) -> Result<(), Error> {
        info!("Clearing & reindexing chain index");

        // create temporary in-memory to wipe directory
        self.chain_index =
            TraitsIndex::create_in_memory(self.config.pending_index_config, self.schema.clone())?;

        // remove and re-create data dir
        std::fs::remove_dir_all(&self.chain_index_dir)?;
        std::fs::create_dir_all(&self.chain_index_dir)?;

        // re-create index, and force re-index of chain
        self.chain_index = TraitsIndex::open_or_create_mmap(
            self.config.chain_index_config,
            self.schema.clone(),
            &self.chain_index_dir,
        )?;
        self.index_chain_new_blocks(data_handle)?;

        self.reindex_pending(data_handle)?;

        Ok(())
    }

    fn index_chain_new_blocks(&mut self, data_handle: &EngineHandle<CS, PS>) -> Result<(), Error> {
        let (_last_chain_block_offset, last_chain_block_height) =
            data_handle.get_chain_last_block()?.ok_or_else(|| {
                Error::Other("Tried to index chain, but it had no blocks in it".to_string())
            })?;

        let last_indexed_block = self.last_indexed_block(&data_handle)?;
        if let Some((_last_indexed_offset, last_indexed_height)) = last_indexed_block {
            if last_chain_block_height - last_indexed_height < self.config.chain_index_min_depth {
                debug!("No new blocks to index from chain. last_chain_block_height={} last_indexed_block_height={}",
                       last_chain_block_height, last_indexed_height
                );
                return Ok(());
            }
        }

        let offset_from = last_indexed_block.map(|(offset, _height)| offset);
        let operations = data_handle.get_chain_operations(offset_from);

        let mut pending_index_mutations = Vec::new();

        let chain_index_min_depth = self.config.chain_index_min_depth;
        let schema = self.schema.clone();
        let chain_index_mutations = operations
            .flat_map(|op| {
                if let EngineOperationStatus::Committed(offset, depth) = op.status {
                    Some((offset, depth, op))
                } else {
                    None
                }
            })
            .filter(|(offset, depth, _op)| {
                *offset > offset_from.unwrap_or(0)
                    && last_chain_block_height - *depth >= chain_index_min_depth
            })
            .flat_map(|(offset, depth, op)| {
                if let Ok(data) = op.as_entry_data() {
                    let mutation = Mutation::from_json_slice(schema.clone(), data).ok()?;
                    Some((offset, depth, op, mutation))
                } else {
                    None
                }
            })
            .map(|(offset, _depth, op, mutation)| {
                pending_index_mutations.push(IndexMutation::DeleteOperation(op.operation_id));

                match mutation {
                    Mutation::PutTrait(trt_mut) => IndexMutation::PutTrait(PutTraitMutation {
                        block_offset: Some(offset),
                        operation_id: op.operation_id,
                        entity_id: trt_mut.entity_id,
                        trt: trt_mut.trt,
                    }),
                    Mutation::DeleteTrait(trt_del) => {
                        IndexMutation::DeleteTrait(trt_del.entity_id, trt_del.trait_id)
                    }
                }
            });

        self.chain_index.apply_mutations(chain_index_mutations)?;
        info!(
            "Indexed {} new operations to chain",
            pending_index_mutations.len()
        );
        self.pending_index
            .apply_mutations(pending_index_mutations.into_iter())?;

        Ok(())
    }

    fn last_indexed_block(
        &self,
        data_handle: &EngineHandle<CS, PS>,
    ) -> Result<Option<(BlockOffset, BlockDepth)>, Error> {
        let last_indexed_offset = self.chain_index.highest_indexed_block()?;

        Ok(last_indexed_offset
            .and_then(|offset| data_handle.get_chain_block_info(offset).ok())
            .and_then(|opt| opt))
    }

    fn handle_pending_operation_new(
        &mut self,
        data_handle: &EngineHandle<CS, PS>,
        operation_id: OperationId,
    ) -> Result<(), Error> {
        let schema = self.schema.clone();
        let operation = data_handle
            .get_pending_operation(operation_id)?
            .expect("Couldn't find operation in data layer for an event we received");
        if let Some(mutation) = Self::pending_operation_to_index_mutation(schema, operation) {
            self.pending_index.apply_mutation(mutation)?;
        }

        Ok(())
    }

    fn pending_operation_to_index_mutation(
        schema: Arc<Schema>,
        operation: EngineOperation,
    ) -> Option<IndexMutation> {
        if let Ok(data) = operation.as_entry_data() {
            let mutation = Mutation::from_json_slice(schema, data).ok()?;
            match mutation {
                Mutation::PutTrait(mutation) => Some(IndexMutation::PutTrait(PutTraitMutation {
                    block_offset: None,
                    operation_id: operation.operation_id,
                    entity_id: mutation.entity_id,
                    trt: mutation.trt,
                })),
                Mutation::DeleteTrait(_mutation) => {
                    // TODO: Special case. We need to index a thombstone

                    None
                }
            }
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::domain::entity::{Record, Trait};
    use crate::domain::schema::tests::create_test_schema;
    use crate::mutation::PutTraitMutation;
    use exocore_data::tests_utils::DataTestCluster;
    use exocore_data::{DirectoryChainStore, MemoryPendingStore};

    #[test]
    fn index_full_pending_to_chain() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut cluster = create_test_cluster()?;

        let config = EntitiesIndexConfig {
            chain_index_min_depth: 1, // index when block is at depth 1 or more
            ..create_test_config()
        };
        let temp = tempdir::TempDir::new("entities_index")?;
        let mut index = EntitiesIndex::open_or_create(temp.path(), config, schema.clone())?;
        handle_engine_events(&mut cluster, &mut index)?;

        // index a few traits, they should now be available from pending index
        insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 0..=9)?;
        handle_engine_events(&mut cluster, &mut index)?;
        let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
        let pending_res = count_results_source(&res, EntityResultSource::Pending);
        let chain_res = count_results_source(&res, EntityResultSource::Chain);
        assert_eq!(pending_res, 10);
        assert_eq!(chain_res, 0);

        // index a few traits, wait for them to be in a block
        insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 10..=19)?;
        handle_engine_events(&mut cluster, &mut index)?;
        let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
        let pending_res = count_results_source(&res, EntityResultSource::Pending);
        let chain_res = count_results_source(&res, EntityResultSource::Chain);
        assert_eq!(pending_res, 10);
        assert_eq!(chain_res, 10);

        Ok(())
    }

    #[test]
    fn reopen_chain_index() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut cluster = create_test_cluster()?;

        let config = EntitiesIndexConfig {
            chain_index_min_depth: 0, // index as soon as new block appear
            ..create_test_config()
        };
        let temp = tempdir::TempDir::new("entities_index")?;

        {
            // index a few traits & make sure it's in the chain index
            let mut index = EntitiesIndex::open_or_create(temp.path(), config, schema.clone())?;
            insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 0..=9)?;
            cluster.wait_next_block_commit(0);
            cluster.clear_received_events(0);
            index.reindex_chain(cluster.get_handle(0))?;
        }

        {
            // index a few traits & make sure it's in the chain index
            let mut index = EntitiesIndex::open_or_create(temp.path(), config, schema.clone())?;

            // we notify the index that the engine is ready
            index.handle_engine_event(cluster.get_handle(0), Event::Started)?;

            // traits should still be indexed
            let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
            assert_eq!(res.results.len(), 10);
        }

        Ok(())
    }

    #[test]
    fn reindex_pending_on_discontinuity() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut cluster = create_test_cluster()?;

        let config = create_test_config();
        let temp = tempdir::TempDir::new("entities_index")?;

        // index traits without indexing them by clearing events
        let mut index = EntitiesIndex::open_or_create(temp.path(), config, schema.clone())?;
        insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 0..=9)?;
        cluster.clear_received_events(0);

        let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
        assert_eq!(res.results.len(), 0);

        // trigger discontinuity, which should force reindex
        index.handle_engine_event(cluster.get_handle(0), Event::StreamDiscontinuity)?;

        // pending is indexed
        let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
        assert_eq!(res.results.len(), 10);

        Ok(())
    }

    #[test]
    fn chain_divergence() -> Result<(), failure::Error> {
        let schema = create_test_schema();
        let mut cluster = create_test_cluster()?;

        let config = EntitiesIndexConfig {
            chain_index_min_depth: 0, // index as soon as new block appear
            ..create_test_config()
        };
        let temp = tempdir::TempDir::new("entities_index")?;

        let mut index = EntitiesIndex::open_or_create(temp.path(), config, schema.clone())?;

        // create 3 blocks worth of traits
        insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 0..=4)?;
        cluster.wait_next_block_commit(0);
        insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 5..=9)?;
        cluster.wait_next_block_commit(0);
        insert_traits_mutation(schema.clone(), &mut cluster, &mut index, 10..=14)?;
        cluster.wait_next_block_commit(0);
        cluster.clear_received_events(0);

        // divergence without anything in index will trigger re-indexation
        index.handle_engine_event(cluster.get_handle(0), Event::ChainDiverged(0))?;
        let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
        assert_eq!(res.results.len(), 15);

        // divergence at an offset not indexed yet will just re-index pending
        let (chain_last_offset, _) = cluster.get_handle(0).get_chain_last_block()?.unwrap();
        index.handle_engine_event(
            cluster.get_handle(0),
            Event::ChainDiverged(chain_last_offset),
        )?;
        let res = index.search(cluster.get_handle(0), &Query::with_trait("contact"))?;
        assert_eq!(res.results.len(), 15);

        // divergence at an offset indexed in chain index will fail
        let res = index.handle_engine_event(cluster.get_handle(0), Event::ChainDiverged(0));
        assert!(res.is_err());

        Ok(())
    }

    #[test]
    fn trait_delete_mutation() -> Result<(), failure::Error> {
        // TODO: thombstone test
        Ok(())
    }

    #[test]
    fn all_traits_delete_mutation() -> Result<(), failure::Error> {
        // TODO: thombstone test
        // TODO: entity should not be returned anymore
        Ok(())
    }

    fn create_test_cluster() -> Result<DataTestCluster, failure::Error> {
        let mut cluster = DataTestCluster::new(1)?;

        cluster.create_node(0)?;
        cluster.create_chain_genesis_block(0);
        cluster.start_engine(0);

        // wait for engine to start
        cluster.collect_events_stream(0);
        cluster.wait_started(0);

        Ok(cluster)
    }

    fn create_test_config() -> EntitiesIndexConfig {
        EntitiesIndexConfig {
            pending_index_config: TraitsIndexConfig {
                indexer_num_threads: Some(1),
                ..TraitsIndexConfig::default()
            },
            chain_index_config: TraitsIndexConfig {
                indexer_num_threads: Some(1),
                ..TraitsIndexConfig::default()
            },
            ..EntitiesIndexConfig::default()
        }
    }

    fn insert_traits_mutation<R: Iterator<Item = i32>>(
        schema: Arc<Schema>,
        cluster: &mut DataTestCluster,
        index: &mut EntitiesIndex<DirectoryChainStore, MemoryPendingStore>,
        range: R,
    ) -> Result<(), failure::Error> {
        let handle = cluster.get_handle(0);
        for i in range {
            let mutation = Mutation::PutTrait(PutTraitMutation {
                entity_id: format!("entity{}", i),
                trt: Trait::new(schema.clone(), "contact")
                    .with_id(format!("trt{}", i))
                    .with_value_by_name("name", format!("name{}", i)),
            });
            index.write_mutation(handle, mutation)?;
        }
        Ok(())
    }

    fn handle_engine_events(
        cluster: &mut DataTestCluster,
        index: &mut EntitiesIndex<
            chain::directory::DirectoryChainStore,
            pending::memory::MemoryPendingStore,
        >,
    ) -> Result<(), Error> {
        while let Some(event) = cluster.pop_received_event(0) {
            index.handle_engine_event(cluster.get_handle(0), event)?;
        }

        Ok(())
    }

    fn count_results_source(results: &EntitiesResults, source: EntityResultSource) -> usize {
        results
            .results
            .iter()
            .filter(|r| r.source == source)
            .count()
    }

}
