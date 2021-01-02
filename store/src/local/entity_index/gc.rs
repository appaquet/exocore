use std::{
    collections::VecDeque,
    sync::{Arc, RwLock},
    time::Duration,
};

use chrono::prelude::*;
use exocore_chain::{block::BlockOffset, chain, operation::OperationId, pending};
use exocore_core::protos::store::MutationRequest;

use crate::{
    entity::{EntityId, TraitId},
    local::mutation_index::MutationMetadata,
    mutation::MutationBuilder,
};

use super::{sort_mutations_commit_time, EntityAggregator, EntityIndex};

const DAY_SECS: u64 = 86_400;

/// Configuration of the entity compactor.
#[derive(Debug, Clone, Copy)]
pub struct GarbageCollectorConfig {
    /// How often the garbage collection process will run. Since garbage collection
    /// doesn't happen on the whole index, but only on entities that got flagged
    /// during search, it is better to run more often than less. The `queue_size`
    /// can be tweaked to control rate of collection.
    pub run_interval: Duration,

    /// After how long do we collect a fully deleted entity.
    pub deleted_entity_collection: Duration,

    /// After how long do we collect a fully deleted trait.
    pub deleted_trait_collection: Duration,

    /// Maximum versions we keep for a trait.
    pub trait_versions_max: usize,

    /// If higher than `trait_versions_max`, a compaction of trait versions
    /// will only happen if the number of versions reaches this value.
    pub trait_versions_leeway: usize,

    /// Size of queue of entities to be compacted.
    pub queue_size: usize,
}

impl Default for GarbageCollectorConfig {
    fn default() -> Self {
        GarbageCollectorConfig {
            run_interval: Duration::from_secs(30),
            deleted_entity_collection: Duration::from_secs(30 * DAY_SECS),
            deleted_trait_collection: Duration::from_secs(30 * DAY_SECS),
            trait_versions_max: 5,
            trait_versions_leeway: 7,
            queue_size: 10,
        }
    }
}

/// The entity garbage collector generates operation deletion mutations on
/// entities that need to be cleaned up from the mutation index.
///
/// Since the chain contains an immutable list of entity mutations, the
/// mutation index contains every mutation that an entity got. When deleting an
/// entity or a trait, we simply create a tombstone to support versioning and to
/// make sure that all nodes have a consistent index. This is also required to
/// make sure that a new node that bootstraps doesn't need to garbage collector
/// the index by itself. By including the deletions in the chain, this new node
/// can delete mutations by operation ids as it indexes them.
///
/// When searching, if the mutation that got returned by the mutation index
/// isn't valid anymore because it got overridden by another mutation, or
/// because the last mutation was a deletion (tombstone on entity or trait), the
/// search result is discarded (but could be returned by a later valid
/// mutations). This eventually create a problem where a lot of mutations get
/// returned by the mutation index to be then discarded by the entity index, up
/// to eventually creating the issue where we hit the maximum number of pages
/// and don't return valid entities.
///
/// There is 3 kind of garbage collections:
/// * Deleted entity: effectively deletes all traits of an entity.
/// * Deleted trait: effectively deletes all versions of a trait of an entity.
/// * Trait: deletes old versions of a trait.
pub struct GarbageCollector {
    config: GarbageCollectorConfig,
    max_entity_deleted_duration: chrono::Duration,
    max_trait_deleted_duration: chrono::Duration,
    inner: Arc<RwLock<Inner>>,
}

impl GarbageCollector {
    pub fn new(config: GarbageCollectorConfig) -> Self {
        let max_entity_deleted_duration =
            chrono::Duration::from_std(config.deleted_entity_collection)
                .expect("Couldn't convert `deleted_entity_collection` to chrono::Duration");

        let max_trait_deleted_duration =
            chrono::Duration::from_std(config.deleted_trait_collection)
                .expect("Couldn't convert `deleted_trait_collection` to chrono::Duration");

        let inner = Arc::new(RwLock::new(Inner {
            config,
            queue: VecDeque::with_capacity(config.queue_size),
        }));

        GarbageCollector {
            config,
            max_entity_deleted_duration,
            max_trait_deleted_duration,
            inner,
        }
    }

    /// Checks if an entity for which we collected its mutation metadata from the index should be added
    /// to the garbage collection queue.
    pub fn maybe_flag_for_collection(&self, entity_id: &str, aggregator: &EntityAggregator) {
        let last_block_offset = if let Some(offset) = aggregator.last_block_offset {
            offset
        } else {
            // we don't collect if operations are only in pending
            return;
        };

        let now = Utc::now();

        if let Some(deletion_date) = &aggregator.deletion_date {
            let elapsed = now.signed_duration_since(*deletion_date);
            if elapsed > self.max_entity_deleted_duration {
                let mut writer = self.inner.write().expect("Fail to acquire inner lock");
                writer.maybe_enqueue(Operation::DeleteEntity(
                    last_block_offset,
                    entity_id.to_string(),
                ));
                return;
            }
        }

        for (trait_id, trait_aggr) in &aggregator.traits {
            if let Some(deletion_date) = &trait_aggr.deletion_date {
                let elapsed = now.signed_duration_since(*deletion_date);
                if elapsed > self.max_trait_deleted_duration {
                    let mut writer = self.inner.write().expect("Fail to acquire inner lock");
                    writer.maybe_enqueue(Operation::DeleteTrait(
                        last_block_offset,
                        entity_id.to_string(),
                        trait_id.clone(),
                    ));
                    continue;
                }
            }

            if trait_aggr.mutation_count > self.config.trait_versions_max {
                let mut writer = self.inner.write().expect("Fail to acquire inner lock");
                writer.maybe_enqueue(Operation::CompactTrait(
                    last_block_offset,
                    entity_id.to_string(),
                    trait_id.clone(),
                ));
            }
        }
    }

    /// Garbage collect the entities currently in queue.
    pub fn run<CS, PS>(&self, index: &EntityIndex<CS, PS>) -> Vec<MutationRequest>
    where
        CS: chain::ChainStore,
        PS: pending::PendingStore,
    {
        let ops = {
            let mut inner = self.inner.write().expect("Fail to acquire inner lock");
            let mut queue = VecDeque::new();
            std::mem::swap(&mut queue, &mut inner.queue);
            queue
        };

        if ops.is_empty() {
            return Vec::new();
        }

        debug!(
            "Starting garbage collection with {} operations...",
            ops.len()
        );
        let mut deletions = Vec::new();
        for op in ops {
            let until_block_offset = op.until_block_offset();
            let entity_id = op.entity_id();

            // get all mutations for entity until block offset where we deemed the entity to
            // be collectable
            let mutations =
                if let Ok(mut_res) = index.chain_index.fetch_entity_mutations(&entity_id) {
                    let mutations = mut_res
                        .mutations
                        .into_iter()
                        .filter(|mutation| match mutation.block_offset {
                            Some(offset) if offset < until_block_offset => true,
                            _ => false,
                        });

                    sort_mutations_commit_time(mutations)
                } else {
                    error!("Couldn't fetch mutations for entity {}", entity_id);
                    continue;
                };

            match op {
                Operation::DeleteEntity(_, entity_id) => {
                    deletions.push(collect_delete_entity(&entity_id, mutations));
                }
                Operation::DeleteTrait(_, entity_id, trait_id) => {
                    deletions.push(collect_delete_trait(&entity_id, &trait_id, mutations));
                }
                Operation::CompactTrait(_, entity_id, trait_id) => {
                    if let Some(deletion) = collect_trait_versions(
                        &entity_id,
                        &trait_id,
                        self.config.trait_versions_max,
                        mutations,
                    ) {
                        deletions.push(deletion);
                    }
                }
            }
        }
        debug!(
            "Garbage collection generated {} deletion operations",
            deletions.len()
        );

        deletions
    }
}

/// Creates a deletion mutation for all operations of the entity.
fn collect_delete_entity<I>(entity_id: &str, mutations: I) -> MutationRequest
where
    I: Iterator<Item = MutationMetadata>,
{
    let operation_ids = mutations.map(|mutation| mutation.operation_id).collect();

    debug!(
        "Creating delete operation to garbage collect deleted entity {}",
        entity_id
    );
    MutationBuilder::new()
        .delete_operations(entity_id, operation_ids)
        .build()
}

/// Creates a deletion mutation for all operations of a trait of an entity.
fn collect_delete_trait<I>(entity_id: &str, trait_id: &str, mutations: I) -> MutationRequest
where
    I: Iterator<Item = MutationMetadata>,
{
    let trait_mutations = filter_trait_mutations(mutations, trait_id);
    let operation_ids = trait_mutations
        .map(|mutation| mutation.operation_id)
        .collect();

    debug!(
        "Creating delete operation to garbage collect deleted trait {} of entity {}",
        trait_id, entity_id
    );
    MutationBuilder::new()
        .delete_operations(entity_id, operation_ids)
        .build()
}

/// Creates a deletion mutation for the N oldest operations of a trait so that
/// we only keep the most recent ones.
fn collect_trait_versions<I>(
    entity_id: &str,
    trait_id: &str,
    max_versions: usize,
    mutations: I,
) -> Option<MutationRequest>
where
    I: Iterator<Item = MutationMetadata>,
{
    let trait_operations: Vec<OperationId> = filter_trait_mutations(mutations, trait_id)
        .map(|mutation| mutation.operation_id)
        .collect();

    if trait_operations.len() <= max_versions {
        return None;
    }

    let to_delete_count = trait_operations.len() - max_versions;
    let operation_ids = trait_operations
        .into_iter()
        .rev()
        .take(to_delete_count)
        .collect();

    debug!(
        "Creating delete operation to garbage collect {} operations of trait {} of entity {}",
        to_delete_count, trait_id, entity_id
    );
    Some(
        MutationBuilder::new()
            .delete_operations(entity_id, operation_ids)
            .build(),
    )
}

fn filter_trait_mutations<'i, I>(
    mutations: I,
    trait_id: &'i str,
) -> impl Iterator<Item = MutationMetadata> + 'i
where
    I: Iterator<Item = MutationMetadata> + 'i,
{
    mutations.filter(move |mutation| match &mutation.mutation_type {
        crate::local::mutation_index::MutationType::TraitPut(put_mut)
            if put_mut.trait_id == trait_id =>
        {
            true
        }
        crate::local::mutation_index::MutationType::TraitTombstone(tomb_trait_id)
            if tomb_trait_id == &trait_id =>
        {
            true
        }
        _ => false,
    })
}

struct Inner {
    config: GarbageCollectorConfig,
    queue: VecDeque<Operation>,
}

impl Inner {
    fn is_full(&self) -> bool {
        self.queue.len() > self.config.queue_size
    }

    fn maybe_enqueue(&mut self, op: Operation) {
        if self.is_full() {
            return;
        }

        self.queue.push_back(op);
    }
}

enum Operation {
    DeleteEntity(BlockOffset, EntityId),
    DeleteTrait(BlockOffset, EntityId, TraitId),
    CompactTrait(BlockOffset, EntityId, TraitId),
}

impl Operation {
    fn entity_id(&self) -> EntityId {
        match self {
            Operation::DeleteEntity(_, entity_id) => entity_id.to_string(),
            Operation::DeleteTrait(_, entity_id, _) => entity_id.to_string(),
            Operation::CompactTrait(_, entity_id, _) => entity_id.to_string(),
        }
    }

    fn until_block_offset(&self) -> BlockOffset {
        match self {
            Operation::DeleteEntity(offset, _) => *offset,
            Operation::DeleteTrait(offset, _, _) => *offset,
            Operation::CompactTrait(offset, _, _) => *offset,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn collect_old_deleted_entity() {
        // TODO:
    }

    #[test]
    fn collect_old_deleted_trait() {
        // TODO:
    }

    #[test]
    fn collect_old_trait_versions() {
        // TODO:
    }
}
