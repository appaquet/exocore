#![allow(dead_code)]
#![allow(unused_variables)]
#![allow(unused_imports)]
#![allow(unused_mut)]

use crate::chain;
use crate::chain::Block;
use crate::engine::{chain_sync, pending_sync, SyncContext};
use crate::pending;

use super::Error;

use crate::chain::BlockOffset;
use exocore_common::node::{Node, NodeID, Nodes};
use exocore_common::serialization::framed::{FrameBuilder, OwnedTypedFrame, TypedFrame};
use exocore_common::serialization::protos::data_chain_capnp::{
    block, block_header, block_signature, block_signatures, operation_block_propose,
    operation_block_refuse, operation_block_sign, operation_entry_new, operation_pending_ignore,
    pending_operation, pending_operation::operation,
};
use exocore_common::serialization::protos::OperationID;
use exocore_common::security::signature::Signature;

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
    phantom: std::marker::PhantomData<(PS, CS)>,
}

impl<PS: pending::Store, CS: chain::Store> CommitManager<PS, CS> {
    pub fn new(node_id: NodeID, config: Config) -> CommitManager<PS, CS> {
        CommitManager {
            node_id,
            config,
            phantom: std::marker::PhantomData,
        }
    }

    pub fn tick(
        &mut self,
        _sync_context: &mut SyncContext,
        _pending_synchronizer: &mut pending_sync::Synchronizer<PS>,
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

        let last_block = chain_store.get_last_block()?.ok_or_else(|| {
            Error::Other("Chain doesn't contain any block. Cannot progress on commits.".to_string())
        })?;
        let next_offset = last_block.next_offset();

        let nb_nodes_concensus = nodes.len() / 2;

        let mut pending_blocks = self.collect_blocks(pending_store, chain_store, nodes)?;
        let mut potential_next_blocks: Vec<&mut PendingBlock> = pending_blocks
            .iter_mut()
            .filter(|block| {
                block.proposal.offset == next_offset && block.refusals.len() < nb_nodes_concensus
            })
            .collect();

        // TODO: Get entries -> block

        // sort potential blocks by current number of signatures, descending
        potential_next_blocks.sort_by(|a, b| a.signatures.len().cmp(&b.signatures.len()).reverse());

        // TODO: But we may have already signed a potential one... We may not be able to advance concensus if each node vote for a different proposal

        if let Some(next_block) = potential_next_blocks.first() {
            let has_my_signature = next_block.contains_node_signature(&self.node_id);
            let has_my_refusal = next_block.contains_node_refusal(&self.node_id);

            if !has_my_signature && !has_my_refusal {
                if self.should_sign_block(next_block) {
                    // TODO: next_block.signatures.push(my_new_signature);
                    // TODO: sign it
                } else {
                    // TODO: refuse it
                }
            }

            if next_block.signatures.len() > nb_nodes_concensus {
                // TODO: chain_store.write_block()
                // TODO: notify the chain_synchronizer
            }
        } else if self.should_propose_block() {
            let operations = self.find_operations_to_commit(pending_store)?;


        }

        // TODO: Check if we can advance the last block mark in pending store

        // TODO: Emit "PendingIgnore" for
        //        - Operations that are now in the chain
        //        - Blocks that got refused after more than X

        Ok(())
    }

    fn should_propose_block(&self) -> bool {
        // TODO: I'm synchronized
        // TODO: I have full access
        // TODO: Last block time + duration + hash(nodeid) % 5 secs
        true
    }

    fn should_sign_block(&self, block: &PendingBlock) -> bool {
        // TODO: Validate everything...
        true
    }

    fn find_operations_to_commit(&self, pending_store: &PS) -> Result<(), Error> {
        // TODO: find all entries that weren't comitted yet

        // TODO: get operations by asc timeline
        // TODO: get operations group for each operation
        // TODO: find all operations that were not committed yet

        // TODO: make sure that they aren't in the chain yet?

        Ok(())
    }

    fn collect_blocks(
        &self,
        pending_store: &PS,
        chain_store: &CS,
        nodes: &Nodes,
    ) -> Result<Vec<PendingBlock>, Error> {
        // first pass to fetch all groups proposal
        let mut groups_id = Vec::new();
        for pending_op in pending_store.operations_iter(..)? {
            if pending_op.operation_type == pending::OperationType::BlockPropose {
                groups_id.push(pending_op.operation_id);
            }
        }

        // then we get all operations for each block proposal
        let mut blocks = Vec::<PendingBlock>::new();
        for group_id in groups_id.iter_mut() {
            if let Some(operations) = pending_store.get_group_operations(*group_id)? {
                let mut entries = Vec::new();
                let mut proposal: Option<PendingBlockProposal> = None;
                let mut signatures = Vec::new();
                let mut refusals = Vec::new();

                for operation in operations.operations {
                    let operation_reader: pending_operation::Reader =
                        operation.frame.get_typed_reader()?;
                    let node_id = operation_reader.get_node_id()?;

                    let operation_type = match operation_reader.get_operation().which()? {
                        pending_operation::operation::Which::BlockPropose(reader) => {
                            let reader: operation_block_propose::Reader = reader?;
                            let block_reader: block::Reader = reader.get_block()?;
                            for entry_header in block_reader.get_entries_header()? {
                                entries.push(entry_header.get_operation_id());
                            }

                            proposal = Some(PendingBlockProposal {
                                node_id: node_id.to_string(),
                                offset: block_reader.get_offset(),
                                operation,
                            })
                        }
                        pending_operation::operation::Which::BlockSign(reader) => {
                            let reader: operation_block_sign::Reader = reader?;
                            let signature: block_signature::Reader = reader.get_signature()?;

                            signatures.push(PendingBlockSignature {
                                node_id: node_id.to_string(),
                                signature: Signature::from_bytes(signature.get_node_signature()?),
                            });
                        }
                        pending_operation::operation::Which::BlockRefuse(reader) => {
                            refusals.push(PendingBlockRefusal {
                                node_id: node_id.to_string(),
                                operation,
                            });
                        }
                        pending_operation::operation::Which::PendingIgnore(_)
                        | pending_operation::operation::Which::EntryNew(_) => {
                            warn!("Found a non-block related operation in block group, which shouldn't be possible (group_id={})", group_id);
                        }
                    };
                }

                if let Some(proposal) = proposal {
                    blocks.push(PendingBlock {
                        group_id: *group_id,
                        proposal,
                        refusals,
                        signatures,
                        entries,
                    })
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

    proposal: PendingBlockProposal,
    refusals: Vec<PendingBlockRefusal>,
    signatures: Vec<PendingBlockSignature>,

    entries: Vec<OperationID>,
}

impl PendingBlock {
    fn contains_node_signature(&self, node_id: &str) -> bool {
        self.signatures
            .iter()
            .find(|sig| sig.node_id == node_id)
            .is_some()
    }

    fn contains_node_refusal(&self, node_id: &str) -> bool {
        self.refusals
            .iter()
            .find(|sig| sig.node_id == node_id)
            .is_some()
    }

    fn to_store_block(&self) -> Result<chain::BlockOwned, Error> {
        let operation_reader: pending_operation::Reader = self.proposal.operation.frame.get_typed_reader()?;

        unimplemented!()
    }
}

struct PendingBlockProposal {
    node_id: NodeID,
    offset: BlockOffset,
    operation: pending::StoredOperation,
}

struct PendingBlockRefusal {
    node_id: NodeID,
    operation: pending::StoredOperation,
}

struct PendingBlockSignature {
    node_id: NodeID,
    signature: Signature,
}
