use super::traits::TraitsIndex;
use crate::domain::entity::{EntityId, FieldValue, Record, Trait, TraitId};
use crate::domain::schema;
use crate::error::Error;
use crate::mutation::Mutation;
use crate::query::*;
use exocore_data::engine::Event;
use exocore_data::EngineHandle;
use exocore_data::{chain, pending};
use serde::ser::Serialize;
use std::path::Path;
use std::sync::Arc;

/*
 TODO:
   - When starting should check the last block indexed (commit is atomic, we shouldn't have half blocks indexed)
   - When a block is committed
        - check what is last block indexed
        - if we have any block that are passed a depth that weren't indexed yet, we index them
        - delete them from in-memory
   - When new stuff in-memory
        - index them
   - When querying, we should always check if we have any new stuff in-memory for given results
*/

///
///
///
struct IndexConfig {}

impl Default for IndexConfig {
    fn default() -> Self {
        IndexConfig {}
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
    pending_index: TraitsIndex,
    chain_index: TraitsIndex,
    phantom: std::marker::PhantomData<(CS, PS)>,
}

impl<CS, PS> EntitiesIndex<CS, PS>
where
    CS: chain::ChainStore,
    PS: pending::PendingStore,
{
    pub fn new(
        index_dir: &Path,
        schema: Arc<schema::Schema>,
    ) -> Result<EntitiesIndex<CS, PS>, Error> {
        let pending_index = TraitsIndex::new_in_memory(schema.clone())?;
        let chain_index = TraitsIndex::new_mmap(schema.clone(), index_dir)?;

        Ok(EntitiesIndex {
            pending_index,
            chain_index,
            phantom: std::marker::PhantomData,
        })
    }

    pub fn query(
        &self,
        data_handle: EngineHandle<CS, PS>,
        query: Query,
    ) -> Result<QueryResults, Error> {
        unimplemented!()
    }

    pub fn write_mutation(
        &mut self,
        data_handle: EngineHandle<CS, PS>,
        mutation: Mutation,
    ) -> Result<(), Error> {
        let json_mutation = mutation.to_json()?;
        data_handle.write_entry_operation(json_mutation.as_bytes())?;
        Ok(())
    }

    pub fn reindex_chain(&mut self, data_handle: EngineHandle<CS, PS>) {
        unimplemented!()
    }

    pub fn reindex_pending(&mut self, data_handle: EngineHandle<CS, PS>) {
        unimplemented!()
    }

    pub fn handle_engine_event(&self, event: Event) {
        match event {
            Event::Started => {
                // TODO: check if need reindex ?
            }
            Event::StreamDiscontinuity => {
                // TODO: pending index, re-index
            }
            Event::PendingOperationNew(_op_id) => {}
            Event::PendingOperationDelete(_op_id) => {}
            Event::ChainBlockNew(_block_offset) => {}
            Event::ChainDiverged(_block_offset) => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use exocore_common::tests_utils::setup_logging;
    use exocore_data::tests_utils::DataTestCluster;

    #[test]
    fn index_chain() -> Result<(), failure::Error> {
        setup_logging();

        let mut cluster = DataTestCluster::new(1)?;

        cluster.create_node(0)?;
        cluster.create_chain_genesis_block(0);
        cluster.start_engine(0);

        // wait for engine to start
        cluster.collect_events_stream(0);
        cluster.wait_started(0);

        std::thread::sleep_ms(5234);

        Ok(())
    }
}
