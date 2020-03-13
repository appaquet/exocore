use crate::block::{Block, BlockOperations, BlockOwned, BlockSignature, BlockSignatures};
use crate::chain;
use crate::engine::{pending_sync, EngineError, Event, SyncContext};
use crate::operation::{Operation, OperationBuilder, OperationId, OperationType};
use crate::pending;
use crate::pending::CommitStatus;
use exocore_core::cell::NodeId;
use exocore_core::cell::{Cell, CellNodes, CellNodesRead};
use exocore_core::time::{Clock, ConsistentTimestamp};
use itertools::Itertools;

use block::{BlockStatus, PendingBlock, PendingBlockRefusal, PendingBlockSignature, PendingBlocks};
pub use config::CommitManagerConfig;
pub use error::CommitManagerError;

mod block;
mod config;
mod error;
#[cfg(test)]
mod tests;

/// Manages commit of pending store's operations to the chain. It does that by
/// monitoring the pending store for incoming block proposal, signing/refusing
/// them or proposing new blocks.
///
/// It also manages cleanup of the pending store, by deleting old operations
/// that were committed to the chain and that are in block with sufficient
/// height.
pub(super) struct CommitManager<PS: pending::PendingStore, CS: chain::ChainStore> {
    config: CommitManagerConfig,
    cell: Cell,
    clock: Clock,
    phantom: std::marker::PhantomData<(PS, CS)>,
}

impl<PS: pending::PendingStore, CS: chain::ChainStore> CommitManager<PS, CS> {
    pub fn new(config: CommitManagerConfig, cell: Cell, clock: Clock) -> CommitManager<PS, CS> {
        CommitManager {
            config,
            cell,
            clock,
            phantom: std::marker::PhantomData,
        }
    }

    /// Tick is called by the Engine at interval to make progress on proposing
    /// blocks, signing / refusing proposed blocks, and committing them to
    /// the chain. We also cleanup the pending store once operations
    /// have passed a certain depth in the chain, which guarantees their
    /// persistence.
    pub fn tick(
        &mut self,
        sync_context: &mut SyncContext,
        pending_synchronizer: &mut pending_sync::PendingSynchronizer<PS>,
        pending_store: &mut PS,
        chain_store: &mut CS,
    ) -> Result<(), EngineError> {
        // find all blocks (proposed, committed, refused, etc.) in pending store
        let mut pending_blocks = PendingBlocks::new(
            &self.config,
            &self.clock,
            &self.cell,
            pending_store,
            chain_store,
        )?;

        // get all potential next blocks sorted by most probable to less probable, and
        // select the best next block
        let potential_next_blocks = pending_blocks.potential_next_blocks();
        let best_potential_next_block = potential_next_blocks.first().map(|b| b.group_id);
        debug!(
            "{}: Tick begins. potential_next_blocks={:?} best_next_block={:?}",
            self.cell.local_node().id(),
            potential_next_blocks,
            best_potential_next_block
        );

        // if we have a next block, we check if we can sign it and commit it
        if let Some(next_block_id) = best_potential_next_block {
            let (has_my_signature, has_my_refusal) = {
                let next_block = pending_blocks.get_block(&next_block_id);
                (next_block.has_my_signature, next_block.has_my_refusal)
            };

            if !has_my_signature && !has_my_refusal {
                if let Ok(should_sign) = self.check_should_sign_block(
                    next_block_id,
                    &pending_blocks,
                    chain_store,
                    pending_store,
                ) {
                    let mut_next_block = pending_blocks.get_block_mut(&next_block_id);
                    if should_sign {
                        self.sign_block(
                            sync_context,
                            pending_synchronizer,
                            pending_store,
                            mut_next_block,
                        )?;
                    } else {
                        self.refuse_block(
                            sync_context,
                            pending_synchronizer,
                            pending_store,
                            mut_next_block,
                        )?;
                    }
                }
            }

            let next_block = pending_blocks.get_block(&next_block_id);
            let valid_signatures = next_block
                .signatures
                .iter()
                .filter(|sig| next_block.validate_signature(&self.cell, sig));

            let nodes = self.cell.nodes();
            if next_block.has_my_signature && nodes.is_quorum(valid_signatures.count()) {
                debug!(
                    "{}: Block has enough signatures, we should commit",
                    self.cell.local_node().id(),
                );
                self.commit_block(sync_context, next_block, pending_store, chain_store)?;
            }
        } else if self.should_propose_block(chain_store, &pending_blocks)? {
            debug!(
                "{}: No current block, and we can propose one",
                self.cell.local_node().id(),
            );
            self.propose_block(
                sync_context,
                &pending_blocks,
                pending_synchronizer,
                pending_store,
                chain_store,
            )?;
        }

        self.maybe_cleanup_pending_store(
            sync_context,
            &pending_blocks,
            pending_store,
            chain_store,
        )?;

        Ok(())
    }

    /// Checks if we should sign a block that was previously proposed. We need
    /// to make sure all operations are valid and not already in the chain
    /// and then validate the hash of the block with local version of the
    /// operations.
    fn check_should_sign_block(
        &self,
        block_id: OperationId,
        pending_blocks: &PendingBlocks,
        chain_store: &CS,
        pending_store: &PS,
    ) -> Result<bool, EngineError> {
        let block = pending_blocks.get_block(&block_id);
        let block_frame = block.proposal.get_block()?;

        // make sure we don't have operations that are already committed
        for operation_id in &block.operations {
            for block_id in pending_blocks
                .operations_blocks
                .get(operation_id)
                .expect("Operation was not in map")
            {
                let op_block = pending_blocks
                    .blocks_status
                    .get(block_id)
                    .expect("Couldn't find block");
                if *op_block == BlockStatus::PastCommitted {
                    info!(
                        "{}: Refusing block {:?} because there is already a committed block at this offset",
                        self.cell.local_node().id(),
                        block
                    );
                    return Ok(false);
                }

                let operation_in_chain = chain_store
                    .get_block_by_operation_id(*operation_id)?
                    .is_some();
                if operation_in_chain {
                    info!(
                        "{}: Refusing block {:?} because it contains operation_id={} already in chain",
                        self.cell.local_node().id(),
                        block,
                        operation_id
                    );
                    return Ok(false);
                }
            }
        }

        // validate hash of operations of block
        let block_operations = Self::get_block_operations(block, pending_store)?.map(|op| op.frame);
        let operations_hash = BlockOperations::hash_operations(block_operations)?;
        let block_header_reader = block_frame.get_reader()?;
        if operations_hash.as_bytes() != block_header_reader.get_operations_hash()? {
            info!(
                "{}: Refusing block {:?} because entries hash didn't match our local hash for block",
                self.cell.local_node().id(),
                block
            );
            return Ok(false);
        }

        Ok(true)
    }

    /// Adds our signature to a given block proposal.
    fn sign_block(
        &self,
        sync_context: &mut SyncContext,
        pending_synchronizer: &mut pending_sync::PendingSynchronizer<PS>,
        pending_store: &mut PS,
        next_block: &mut PendingBlock,
    ) -> Result<(), EngineError> {
        let local_node = self.cell.local_node();

        let operation_id = self.clock.consistent_time(&local_node);
        let signature_frame_builder = OperationBuilder::new_signature_for_block(
            next_block.group_id,
            operation_id.into(),
            local_node.id(),
            &next_block.proposal.get_block()?,
        )?;

        let signature_operation = signature_frame_builder.sign_and_build(&local_node)?;

        let signature_reader = signature_operation.get_operation_reader()?;
        let pending_signature = PendingBlockSignature::from_operation(signature_reader)?;
        debug!(
            "{}: Signing block {:?}",
            self.cell.local_node().id(),
            next_block,
        );
        next_block.add_my_signature(pending_signature);

        pending_synchronizer.handle_new_operation(
            sync_context,
            pending_store,
            signature_operation,
        )?;

        Ok(())
    }

    /// Adds our refusal to a given block proposal (ex: it's not valid)
    fn refuse_block(
        &self,
        sync_context: &mut SyncContext,
        pending_synchronizer: &mut pending_sync::PendingSynchronizer<PS>,
        pending_store: &mut PS,
        next_block: &mut PendingBlock,
    ) -> Result<(), EngineError> {
        let local_node = self.cell.local_node();

        let operation_id = self.clock.consistent_time(&local_node);

        let refusal_builder = OperationBuilder::new_refusal(
            next_block.group_id,
            operation_id.into(),
            local_node.id(),
        )?;
        let refusal_operation = refusal_builder.sign_and_build(&local_node)?;

        let refusal_reader = refusal_operation.get_operation_reader()?;
        let pending_refusal = PendingBlockRefusal::from_operation(refusal_reader)?;

        next_block.add_my_refusal(pending_refusal);

        pending_synchronizer.handle_new_operation(
            sync_context,
            pending_store,
            refusal_operation,
        )?;

        Ok(())
    }

    /// Checks if we need to propose a new block, based on when the last block
    /// was created and how many operations are in the store.
    fn should_propose_block(
        &self,
        chain_store: &CS,
        pending_blocks: &PendingBlocks,
    ) -> Result<bool, EngineError> {
        let local_node = self.cell.local_node();
        if !local_node.has_full_access() {
            return Ok(false);
        }

        let nodes = self.cell.nodes();
        let now = self.clock.consistent_time(local_node);
        if is_node_commit_turn(&nodes, local_node.id(), now, &self.config)? {
            // number of operations in store minus number of operations in blocks ~=
            // non-committed
            let approx_non_committed_operations = pending_blocks
                .entries_operations_count
                .saturating_sub(pending_blocks.operations_blocks.len());

            if approx_non_committed_operations >= self.config.commit_maximum_pending_store_count {
                debug!(
                    "{}: Enough operations ({} >= {}) to commit & it's my turn. Proposing one.",
                    local_node.id(),
                    approx_non_committed_operations,
                    self.config.commit_maximum_pending_store_count
                );
                Ok(true)
            } else {
                let previous_block = chain_store
                    .get_last_block()?
                    .ok_or(EngineError::UninitializedChain)?;
                let prev_block_op_id = previous_block.get_proposed_operation_id()?;
                let prev_block_time = ConsistentTimestamp::from(prev_block_op_id);
                let previous_block_elapsed = if let Some(elapsed) = now - prev_block_time {
                    elapsed
                } else {
                    return Ok(false);
                };

                if previous_block_elapsed >= self.config.commit_maximum_interval {
                    debug!(
                        "{}: Enough operations to commit & it's my turn. Will propose a block.",
                        local_node.id()
                    );
                    Ok(true)
                } else {
                    Ok(false)
                }
            }
        } else {
            Ok(false)
        }
    }

    /// Creates a new block proposal with operations currently in the store.
    fn propose_block(
        &self,
        sync_context: &mut SyncContext,
        pending_blocks: &PendingBlocks,
        pending_synchronizer: &mut pending_sync::PendingSynchronizer<PS>,
        pending_store: &mut PS,
        chain_store: &mut CS,
    ) -> Result<(), EngineError> {
        let local_node = self.cell.local_node();
        let previous_block = chain_store
            .get_last_block()?
            .ok_or(EngineError::UninitializedChain)?;

        let block_operations = pending_store
            .operations_iter(..)?
            .filter(|operation| operation.operation_type == OperationType::Entry)
            .filter(|operation| {
                // check if operation was committed to any previous block
                let operation_is_committed = pending_blocks
                    .operations_blocks
                    .get(&operation.operation_id)
                    .map_or(false, |blocks| {
                        blocks.iter().any(|block| {
                            let block_status = pending_blocks
                                .blocks_status
                                .get(block)
                                .expect("Couldn't find status of a current block");
                            *block_status == BlockStatus::PastCommitted
                        })
                    });

                let operation_in_chain = chain_store
                    .get_block_by_operation_id(operation.operation_id)
                    .ok()
                    .and_then(|operation| operation)
                    .is_some();

                !operation_is_committed && !operation_in_chain
            })
            .sorted_by_key(|operation| operation.operation_id)
            .map(|operation| operation.frame);

        let block_operations = BlockOperations::from_operations(block_operations)?;
        let block_operation_id = self.clock.consistent_time(&local_node);
        let block = BlockOwned::new_with_prev_block(
            &self.cell,
            &previous_block,
            block_operation_id.into(),
            block_operations,
        )?;
        if block.operations_iter()?.next().is_none() {
            debug!("No operations need to be committed, so won't be proposing any block");
            return Ok(());
        }

        let block_proposal_frame_builder = OperationBuilder::new_block_proposal(
            block_operation_id.into(),
            local_node.id(),
            &block,
        )?;
        let block_proposal_operation = block_proposal_frame_builder.sign_and_build(&local_node)?;

        debug!(
            "{}: Proposed block at offset={} operation_id={:?}",
            self.cell.local_node().id(),
            previous_block.next_offset(),
            block_operation_id,
        );
        pending_synchronizer.handle_new_operation(
            sync_context,
            pending_store,
            block_proposal_operation,
        )?;

        Ok(())
    }

    /// Commits (write) the given block to the chain.
    fn commit_block(
        &self,
        sync_context: &mut SyncContext,
        next_block: &PendingBlock,
        pending_store: &mut PS,
        chain_store: &mut CS,
    ) -> Result<(), EngineError> {
        let block_frame = next_block.proposal.get_block()?;
        let block_header_reader = block_frame.get_reader()?;

        let block_offset = next_block.proposal.offset;
        let block_height = block_header_reader.get_height();

        // fetch block's operations from the pending store
        let block_operations =
            Self::get_block_operations(next_block, pending_store)?.map(|operation| operation.frame);

        // make sure that the hash of operations is same as defined by the block
        // this should never happen since we wouldn't have signed the block if hash
        // didn't match
        let block_operations = BlockOperations::from_operations(block_operations)?;
        if block_operations.multihash_bytes() != block_header_reader.get_operations_hash()? {
            return Err(EngineError::Fatal(
                "Block hash for local entries didn't match block hash, but was previously signed"
                    .to_string(),
            ));
        }

        // build signatures frame
        let signatures = next_block
            .signatures
            .iter()
            .filter_map(|pending_signature| {
                if next_block.validate_signature(&self.cell, pending_signature) {
                    Some(BlockSignature::new(
                        pending_signature.node_id.clone(),
                        pending_signature.signature.clone(),
                    ))
                } else {
                    None
                }
            })
            .collect::<Vec<_>>();
        let block_signatures = BlockSignatures::new_from_signatures(signatures);
        let signatures_frame =
            block_signatures.to_frame_for_existing_block(&block_header_reader)?;

        // finally build the frame
        let block = BlockOwned::new(
            block_offset,
            block_frame.to_owned(),
            block_operations.data().to_vec(),
            signatures_frame,
        );

        debug!(
            "{}: Writing new block to chain: {:?}",
            self.cell.local_node().id(),
            next_block
        );
        chain_store.write_block(&block)?;
        for operation_id in block_operations.operations_id() {
            pending_store.update_operation_commit_status(
                operation_id,
                CommitStatus::Committed(block_offset, block_height),
            )?;
        }
        sync_context.push_event(Event::NewChainBlock(next_block.proposal.offset));

        Ok(())
    }

    /// Retrieves from the pending store all operations that are in the given
    /// block
    fn get_block_operations(
        next_block: &PendingBlock,
        pending_store: &PS,
    ) -> Result<impl Iterator<Item = pending::StoredOperation>, EngineError> {
        let operations = next_block
            .operations
            .iter()
            .map(|operation| {
                pending_store
                    .get_operation(*operation)
                    .map_err(Into::into)
                    .and_then(|op| {
                        op.ok_or_else(|| CommitManagerError::MissingOperation(*operation).into())
                    })
            })
            .collect::<Result<Vec<_>, EngineError>>()? // collect automatically flatten result into Result<Vec<_>>
            .into_iter()
            .sorted_by_key(|operation| operation.operation_id);

        Ok(operations)
    }

    /// Cleanups all operations that have been committed to the chain and that
    /// are deep enough to be considered impossible to be removed (i.e.
    /// there are no plausible fork)
    fn maybe_cleanup_pending_store(
        &self,
        sync_context: &mut SyncContext,
        pending_blocks: &PendingBlocks,
        pending_store: &mut PS,
        chain_store: &CS,
    ) -> Result<(), EngineError> {
        let last_stored_block = chain_store
            .get_last_block()?
            .ok_or(EngineError::UninitializedChain)?;
        let last_stored_block_height = last_stored_block.get_height()?;

        // cleanup all blocks and their operations that are committed or refused with
        // enough depth
        for (group_id, block) in &pending_blocks.blocks {
            if block.status == BlockStatus::PastCommitted
                || block.status == BlockStatus::PastRefused
            {
                let block_frame = block.proposal.get_block()?;
                let block_header_reader = block_frame.get_reader()?;

                let block_offset = block_header_reader.get_offset();
                let block_height = block_header_reader.get_height();

                let height_diff = last_stored_block_height - block_height;
                if height_diff >= self.config.operations_cleanup_after_block_depth {
                    debug!(
                        "Block {:?} can be cleaned up (last_stored_block_height={})",
                        block, last_stored_block_height
                    );

                    // delete the block & related operations (sigs, refusals, etc.)
                    pending_store.delete_operation(*group_id)?;

                    // delete operations of the block if they were committed, but not refused
                    if block.status == BlockStatus::PastCommitted {
                        for operation_id in &block.operations {
                            pending_store.delete_operation(*operation_id)?;
                        }
                    }

                    // update the sync state so that the `PendingSynchronizer` knows what was last
                    // block to get cleaned
                    sync_context.sync_state.pending_last_cleanup_block =
                        Some((block_offset, block_height));
                }
            }
        }

        // get approx number of operations that are not associated with block
        let approx_non_committed_operations = pending_blocks
            .entries_operations_count
            .saturating_sub(pending_blocks.operations_blocks.len());

        // check for dangling operations, which are operations that are already in the
        // chain but not in any blocks that are in pending store. They probably
        // got re-added to the pending store by a node that was out of sync
        if approx_non_committed_operations > 0 {
            let mut operations_to_delete = Vec::new();
            for operation in pending_store.operations_iter(..)? {
                let is_in_block = pending_blocks
                    .operations_blocks
                    .contains_key(&operation.operation_id);
                if !is_in_block {
                    if let Some(block) =
                        chain_store.get_block_by_operation_id(operation.operation_id)?
                    {
                        let block_height = block.get_height()?;
                        let block_depth = last_stored_block_height - block_height;
                        if block_depth >= self.config.operations_cleanup_after_block_depth {
                            operations_to_delete.push(operation.operation_id);
                        }
                    }
                }
            }

            if !operations_to_delete.is_empty() {
                debug!(
                    "Deleting {} dangling operations from pending store",
                    operations_to_delete.len()
                );
                for operation_id in operations_to_delete {
                    pending_store.delete_operation(operation_id)?;
                }
            }
        }

        Ok(())
    }
}

/// In order to prevent nodes to commit new blocks all the same time resulting
/// in splitting the vote, we make nodes propose blocks in turns.
///
/// Turns are calculated by sorting nodes by their node ids, and then finding
/// out who's turn it is based on current time.
fn is_node_commit_turn(
    nodes: &CellNodesRead,
    my_node_id: &NodeId,
    now: ConsistentTimestamp,
    config: &CommitManagerConfig,
) -> Result<bool, EngineError> {
    let nodes_iter = nodes.iter();
    let sorted_nodes = nodes_iter
        .all()
        .sorted_by_key(|node| node.id().to_str())
        .collect_vec();
    let my_node_position = sorted_nodes
        .iter()
        .position(|node| node.id() == my_node_id)
        .ok_or(EngineError::MyNodeNotFound)? as u64;

    let commit_interval = config.commit_maximum_interval.as_nanos() as f64;
    let epoch = (now.0 as f64 / commit_interval as f64).floor() as u64;
    let node_turn = epoch % (sorted_nodes.len() as u64);
    Ok(node_turn == my_node_position)
}
