#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unused_mut)]

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use itertools::Itertools;

use exocore_common::node::{Node, NodeID, Nodes};
use exocore_common::security::signature::Signature;
use exocore_common::serialization::framed::{
    FrameBuilder, OwnedTypedFrame, TypedFrame, TypedSliceFrame,
};
use exocore_common::serialization::protos::data_chain_capnp::{
    block, block_entry_header, block_header, block_signature, block_signatures,
    operation_block_propose, operation_block_refuse, operation_block_sign, operation_entry_new,
    operation_pending_ignore, pending_operation, pending_operation::operation,
};
use exocore_common::serialization::protos::OperationID;
use exocore_common::time::Clock;

use crate::chain;
use crate::chain::Block;
use crate::chain::BlockOffset;
use crate::engine::{chain_sync, pending_sync, SyncContext};
use crate::pending;
use crate::pending::OperationType;
use crate::pending::OperationType::BlockSign;

use super::Error;

///
///
///
#[derive(Copy, Clone, Debug)]
pub(super) struct Config {}

impl Default for Config {
    fn default() -> Self {
        Config {}
    }
}

///
///
///
pub(super) struct CommitManager<PS: pending::Store, CS: chain::Store> {
    node_id: NodeID,
    config: Config,
    clock: Clock,
    phantom: std::marker::PhantomData<(PS, CS)>,
}

impl<PS: pending::Store, CS: chain::Store> CommitManager<PS, CS> {
    pub fn new(node_id: NodeID, config: Config, clock: Clock) -> CommitManager<PS, CS> {
        CommitManager {
            node_id,
            config,
            clock,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn tick(
        &mut self,
        sync_context: &mut SyncContext,
        pending_synchronizer: &mut pending_sync::Synchronizer<PS>,
        pending_store: &mut PS,
        chain_synchronizer: &mut chain_sync::Synchronizer<CS>,
        chain_store: &mut CS,
        nodes: &Nodes,
    ) -> Result<(), Error> {
        // TODO: The chain sync needs to make sure it keeps this synchronized state
        if chain_synchronizer.status() != chain_sync::Status::Synchronized {
            // we need to be synchronized in order to do any progress
            return Ok(());
        }

        let nb_nodes_concensus = nodes.len() / 2;

        let last_stored_block = chain_store.get_last_block()?.ok_or_else(|| {
            Error::Other("Chain doesn't contain any block. Cannot progress on commits.".to_string())
        })?;
        let next_offset = last_stored_block.next_offset();

        // find blocks
        let mut pending_blocks =
            self.collect_blocks(&last_stored_block, pending_store, chain_store, nodes)?;

        // find operations that are in blocks
        let mut operations_blocks: HashMap<OperationID, HashSet<OperationID>> = HashMap::new();
        for block in pending_blocks.values() {
            for operation_id in &block.operations {
                let entry = operations_blocks
                    .entry(*operation_id)
                    .or_insert_with(HashSet::new);
                entry.insert(block.group_id);
            }
        }

        // collect current blocks status
        let mut blocks_status = HashMap::new();
        for (block_group_id, block) in &pending_blocks {
            blocks_status.insert(*block_group_id, block.status);
        }

        // we sort potential next blocks by which block has better potential to become a block
        let mut potential_next_blocks = pending_blocks
            .values_mut()
            .filter(|block| block.status == BlockStatus::NextPotential)
            .sorted_by(|a, b| Self::compare_potential_next_block(a, b))
            .collect::<Vec<_>>();

        // TODO: But we may have already signed a potential one... We may not be able to advance concensus if each node vote for a different proposal

        if let Some(ref mut next_block) = potential_next_blocks.first_mut() {
            if !next_block.has_my_signature && !next_block.has_my_refusal {
                if self.check_should_sign_block(next_block, &blocks_status, &operations_blocks) {
                    self.sign_block(
                        sync_context,
                        pending_synchronizer,
                        pending_store,
                        nodes,
                        next_block,
                    )?;
                } else {
                    self.refuse_block(
                        sync_context,
                        pending_synchronizer,
                        pending_store,
                        nodes,
                        next_block,
                    )?;
                }
            }

            if next_block.signatures.len() > nb_nodes_concensus {
                // TODO: chain_store.write_block()
                // TODO: notify the chain_synchronizer
            }
        } else if self.should_propose_block() {
            debug!("No current block, and we can propose one");
            self.create_block(
                sync_context,
                &operations_blocks,
                &blocks_status,
                pending_synchronizer,
                pending_store,
                nodes,
            )?;
        }

        // TODO: Check if we can advance the last block mark in pending store

        // TODO: Emit "PendingIgnore" for
        //        - Operations that are now in the chain
        //        - Blocks that got refused after more than X

        // TODO: Cleanup committed stuff OR ignored stuff

        Ok(())
    }

    fn compare_potential_next_block(a: &PendingBlock, b: &PendingBlock) -> Ordering {
        if a.has_my_signature {
            return Ordering::Greater;
        } else if b.has_my_signature {
            return Ordering::Less;
        }

        if a.signatures.len() > b.signatures.len() {
            return Ordering::Greater;
        } else if a.signatures.len() < b.signatures.len() {
            return Ordering::Less;
        }

        // fallback to operation id, which is time ordered
        a.group_id.cmp(&b.group_id)
    }

    fn should_propose_block(&self) -> bool {
        // TODO: I'm synchronized
        // TODO: I have full access
        // TODO: Last block time + duration + hash(nodeid) % 5 secs
        //       - Perhaps we should take current time into consideration so that we don't have 2 nodes proposing at the timeout
        true
    }

    fn check_should_sign_block(
        &self,
        block: &PendingBlock,
        blocks_status: &HashMap<OperationID, BlockStatus>,
        operations_blocks: &HashMap<OperationID, HashSet<OperationID>>,
    ) -> bool {
        // TODO: Hash operations
        // TODO: Validate block hash

        // make sure we don't have operations that are already committed
        for operation in &block.operations {
            for block_id in operations_blocks
                .get(operation)
                .expect("Operation was not in map")
            {
                let op_block = blocks_status.get(block_id).expect("Couldn't find block");
                if *op_block == BlockStatus::PastCommitted {
                    return false;
                }

                // TODO: When chain will be indexed, we need to check if they aren't in the chain for real
            }
        }

        true
    }

    fn sign_block(
        &mut self,
        sync_context: &mut SyncContext,
        pending_synchronizer: &mut pending_sync::Synchronizer<PS>,
        pending_store: &mut PS,
        nodes: &Nodes,
        next_block: &mut PendingBlock,
    ) -> Result<(), Error> {
        let my_node = nodes
            .get(&self.node_id)
            .ok_or_else(|| Error::Other("Couldn't find my node in nodes list".to_string()))?;

        let block_reader = next_block.proposal.get_block()?;

        let operation_id = self.clock.consistent_time(&my_node);
        let signature_frame_builder = pending::PendingOperation::new_signature_for_block(
            next_block.group_id,
            operation_id,
            &self.node_id,
            block_reader,
        )?;

        let signature_frame = signature_frame_builder.as_owned_framed(my_node.frame_signer())?;
        let signature_reader = signature_frame.get_typed_reader()?;
        let pending_signature = PendingBlockSignature::from_pending_operation(signature_reader)?;

        next_block.add_my_signature(pending_signature);

        pending_synchronizer.handle_new_operation(
            sync_context,
            nodes,
            pending_store,
            signature_frame,
        )?;

        Ok(())
    }

    fn refuse_block(
        &mut self,
        sync_context: &mut SyncContext,
        pending_synchronizer: &mut pending_sync::Synchronizer<PS>,
        pending_store: &mut PS,
        nodes: &Nodes,
        next_block: &mut PendingBlock,
    ) -> Result<(), Error> {
        let my_node = nodes
            .get(&self.node_id)
            .ok_or_else(|| Error::Other("Couldn't find my node in nodes list".to_string()))?;

        let operation_id = self.clock.consistent_time(&my_node);

        let refusal_frame_builder = pending::PendingOperation::new_refusal(
            next_block.group_id,
            operation_id,
            &self.node_id,
        )?;
        let refusal_frame = refusal_frame_builder.as_owned_framed(my_node.frame_signer())?;
        let refusal_reader = refusal_frame.get_typed_reader()?;
        let pending_refusal = PendingBlockRefusal::from_pending_operation(refusal_reader)?;

        next_block.add_my_refusal(pending_refusal);

        pending_synchronizer.handle_new_operation(
            sync_context,
            nodes,
            pending_store,
            refusal_frame,
        )?;

        Ok(())
    }

    fn create_block(
        &mut self,
        sync_context: &mut SyncContext,
        operations_blocks: &HashMap<OperationID, HashSet<OperationID>>,
        blocks_status: &HashMap<OperationID, BlockStatus>,
        pending_synchronizer: &mut pending_sync::Synchronizer<PS>,
        pending_store: &mut PS,
        nodes: &Nodes,
    ) -> Result<(), Error> {
        let block_operations = pending_store
            .operations_iter(..)?
            .filter(|operation| {
                // only include new entries or pending ignore entries
                match operation.operation_type {
                    OperationType::EntryNew | OperationType::PendingIgnore => true,
                    _ => false,
                }
            })
            .filter(|operation| {
                // check if operation was committed to any previous block
                let operation_is_committed = operations_blocks
                    .get(&operation.operation_id)
                    .map(|blocks| {
                        blocks.iter().any(|block| {
                            let block_status = blocks_status
                                .get(block)
                                .expect("Couldn't find status of a current block");
                            *block_status == BlockStatus::PastCommitted
                        })
                    })
                    .unwrap_or(false);

                // TODO: Double-check if it's in the chain

                !operation_is_committed
            });

        for operation in block_operations {
            debug!(
                "Alright, we can include operation {}",
                operation.operation_id
            );
        }

        // TODO: get operations by asc timeline
        // TODO: get operations group for each operation
        // TODO: find all entries that weren't comitted yet

        Ok(())
    }

    fn collect_blocks(
        &self,
        last_stored_block: &chain::BlockRef,
        pending_store: &PS,
        chain_store: &CS,
        nodes: &Nodes,
    ) -> Result<HashMap<OperationID, PendingBlock>, Error> {
        // first pass to fetch all groups proposal
        let mut groups_id = Vec::new();
        for pending_op in pending_store.operations_iter(..)? {
            if pending_op.operation_type == pending::OperationType::BlockPropose {
                groups_id.push(pending_op.operation_id);
            }
        }

        let nb_nodes_concensus = nodes.len() / 2;
        let next_offset = last_stored_block.next_offset();

        // then we get all operations for each block proposal
        let mut blocks = HashMap::<OperationID, PendingBlock>::new();
        for group_id in groups_id.iter_mut() {
            if let Some(group_operations) = pending_store.get_group_operations(*group_id)? {
                let mut operations = Vec::new();
                let mut proposal: Option<PendingBlockProposal> = None;
                let mut signatures = Vec::new();
                let mut refusals = Vec::new();

                for operation in group_operations.operations {
                    let operation_reader: pending_operation::Reader =
                        operation.frame.get_typed_reader()?;
                    let node_id = operation_reader.get_node_id()?;

                    match operation_reader.get_operation().which()? {
                        pending_operation::operation::Which::BlockPropose(reader) => {
                            let reader: operation_block_propose::Reader = reader?;
                            let block_frame =
                                TypedSliceFrame::<block::Owned>::new(reader.get_block()?)?;
                            let block_reader: block::Reader = block_frame.get_typed_reader()?;

                            for entry_header in block_reader.get_entries_header()? {
                                operations.push(entry_header.get_operation_id());
                            }

                            proposal = Some(PendingBlockProposal {
                                node_id: node_id.to_string(),
                                offset: block_reader.get_offset(),
                                operation,
                            })
                        }
                        pending_operation::operation::Which::BlockSign(_reader) => {
                            signatures.push(PendingBlockSignature::from_pending_operation(
                                operation_reader,
                            )?);
                        }
                        pending_operation::operation::Which::BlockRefuse(reader) => {
                            refusals.push(PendingBlockRefusal::from_pending_operation(
                                operation_reader,
                            )?);
                        }
                        pending_operation::operation::Which::PendingIgnore(_)
                        | pending_operation::operation::Which::EntryNew(_) => {
                            warn!("Found a non-block related operation in block group, which shouldn't be possible (group_id={})", group_id);
                        }
                    };
                }

                if let Some(proposal) = proposal {
                    let has_my_refusal = refusals.iter().any(|sig| sig.node_id == self.node_id);
                    let has_my_signature = signatures.iter().any(|sig| sig.node_id == self.node_id);

                    let status = match chain_store.get_block(proposal.offset).ok() {
                        Some(block) => {
                            let block_reader: block::Reader = block.block.get_typed_reader()?;
                            if block_reader.get_proposed_operation_id() == *group_id {
                                BlockStatus::PastCommitted
                            } else {
                                BlockStatus::PastRefused
                            }
                        }
                        None => {
                            if proposal.offset < next_offset {
                                // means it was a proposed block for a diverged chain
                                BlockStatus::PastRefused
                            } else if refusals.len() >= nb_nodes_concensus || has_my_refusal {
                                BlockStatus::NextRefused
                            } else {
                                BlockStatus::NextPotential
                            }
                        }
                    };

                    blocks.insert(
                        *group_id,
                        PendingBlock {
                            group_id: *group_id,
                            status,

                            proposal,
                            refusals,
                            signatures,

                            has_my_refusal,
                            has_my_signature,

                            operations,
                        },
                    );
                } else {
                    return Err(Error::Fatal(format!(
                        "Couldn't find block proposal in group operations for block group_id={}",
                        group_id
                    )));
                }
            } else {
                warn!("Didn't have any operations for block proposal with group_id={}, which shouldn't be possible", group_id);
            }
        }

        Ok(blocks)
    }
}

///
///
///
struct PendingBlock {
    group_id: OperationID,
    status: BlockStatus,

    proposal: PendingBlockProposal,
    refusals: Vec<PendingBlockRefusal>,
    signatures: Vec<PendingBlockSignature>,
    has_my_refusal: bool,
    has_my_signature: bool,

    operations: Vec<OperationID>,
}

impl PendingBlock {
    fn add_my_signature(&mut self, signature: PendingBlockSignature) {
        self.signatures.push(signature);
        self.has_my_signature = true;
    }

    fn add_my_refusal(&mut self, refusal: PendingBlockRefusal) {
        self.refusals.push(refusal);
        self.has_my_refusal = true;
    }

    fn to_store_block(&self) -> Result<chain::BlockOwned, Error> {
        let operation_reader: pending_operation::Reader =
            self.proposal.operation.frame.get_typed_reader()?;

        unimplemented!()
    }
}

#[derive(PartialEq, Debug, Clone, Copy)]
enum BlockStatus {
    PastRefused,
    PastCommitted,
    NextPotential,
    NextRefused,
}

struct PendingBlockProposal {
    node_id: NodeID,
    offset: BlockOffset,
    operation: pending::StoredOperation,
}

impl PendingBlockProposal {
    fn get_block(&self) -> Result<TypedSliceFrame<block::Owned>, Error> {
        let operation_reader: pending_operation::Reader =
            self.operation.frame.get_typed_reader()?;
        let inner_operation: pending_operation::operation::Reader =
            operation_reader.get_operation();
        match inner_operation.which()? {
            pending_operation::operation::Which::BlockPropose(block_prop) => {
                let block_prop_reader: operation_block_propose::Reader = block_prop?;
                let frame = TypedSliceFrame::new(block_prop_reader.get_block()?)?;
                Ok(frame)
            }
            _ => Err(Error::Other(
                "Expected block sign pending op to create block signature, but got something else"
                    .to_string(),
            )),
        }
    }
}

struct PendingBlockRefusal {
    node_id: NodeID,
}

impl PendingBlockRefusal {
    fn from_pending_operation(
        operation_reader: pending_operation::Reader,
    ) -> Result<PendingBlockRefusal, Error> {
        let inner_operation: pending_operation::operation::Reader =
            operation_reader.get_operation();
        match inner_operation.which()? {
            pending_operation::operation::Which::BlockRefuse(sig) => {
                let node_id = operation_reader.get_node_id()?.to_string();
                Ok(PendingBlockRefusal { node_id })
            }
            _ => Err(Error::Other(
                "Expected block refuse pending op to create block refusal, but got something else"
                    .to_string(),
            )),
        }
    }
}

struct PendingBlockSignature {
    node_id: NodeID,
    signature: Signature,
}

impl PendingBlockSignature {
    fn from_pending_operation(
        operation_reader: pending_operation::Reader,
    ) -> Result<PendingBlockSignature, Error> {
        let inner_operation: pending_operation::operation::Reader =
            operation_reader.get_operation();
        match inner_operation.which()? {
            pending_operation::operation::Which::BlockSign(sig) => {
                let op_signature_reader: operation_block_sign::Reader = sig?;
                let signature_reader: block_signature::Reader =
                    op_signature_reader.get_signature()?;

                let node_id = signature_reader.get_node_id()?.to_string();
                let signature = Signature::from_bytes(signature_reader.get_node_signature()?);

                Ok(PendingBlockSignature { node_id, signature })
            }
            _ => Err(Error::Other(
                "Expected block sign pending op to create block signature, but got something else"
                    .to_string(),
            )),
        }
    }
}
