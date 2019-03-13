#![allow(dead_code)]

use std::collections::HashMap;
use std::time::{Duration, Instant};

use capnp::traits::ToU16;
use exocore_common::node::{Node, NodeID, Nodes};
use exocore_common::serialization::framed::{
    FrameBuilder, OwnedTypedFrame, SignedFrame, TypedFrame, TypedSliceFrame,
};
use exocore_common::serialization::protos::data_chain_capnp::{
    block, block_header, block_signatures,
};
use exocore_common::serialization::protos::data_transport_capnp::{
    chain_sync_request, chain_sync_request::RequestedDetails, chain_sync_response,
};
use exocore_common::serialization::{capnp, framed};

use crate::chain;
use crate::chain::{BlockOffset, Store, StoredBlock};

///
/// Synchronizer's configuration
///
#[derive(Clone, Debug)]
struct Config {
    base_request_timeout: Duration,
    headers_sync_begin_count: chain::BlockOffset,
    headers_sync_end_count: chain::BlockOffset,
    headers_sync_sampled_count: chain::BlockOffset,
    blocks_max_send_size: usize,
}

impl Default for Config {
    fn default() -> Self {
        Config {
            base_request_timeout: Duration::from_secs(5),
            headers_sync_begin_count: 5,
            headers_sync_end_count: 5,
            headers_sync_sampled_count: 10,
            blocks_max_send_size: 50 * 1024,
        }
    }
}

///
/// Synchronizes the local chain against remote nodes' chain.
///
/// It achieves synchronization in 3 stages:
///
/// 1) Gather knowledge about remote nodes' chain metadata (last block, last common block).
///    During this stage, the status is 'Unknown'
///
/// 2) Once we have knowledge of the majority of node, we find the leader node with the longest chain.
///    Taking the longest chain is valid, since this node could not have made progress without a majority
///    of nodes signing the latest blocks.
///
/// 3) Download the missing blocks from the leader, starting from the latest common block, which
///    is our common ancestor.
///    During this stage, the status is 'Downloading'
///
/// 4) Once fully downloaded, we keep asking for other nodes' metadata to make sure we progress and
///    leadership hasn't changed.
///
struct Synchronizer<CS: Store> {
    node_id: NodeID,
    config: Config,
    nodes_info: HashMap<NodeID, NodeSyncInfo>,
    status: Status,
    current_leader: Option<NodeID>,
    phantom: std::marker::PhantomData<CS>,
}

#[derive(Debug, PartialEq)]
enum Status {
    Unknown,
    Downloading,
    Synchronized,
}

impl<CS: Store> Synchronizer<CS> {
    pub fn new(node_id: NodeID, config: Config) -> Synchronizer<CS> {
        Synchronizer {
            node_id,
            config,
            status: Status::Unknown,
            nodes_info: HashMap::new(),
            current_leader: None,
            phantom: std::marker::PhantomData,
        }
    }

    ///
    /// Called at interval to make progress on the synchronization. Depending on the current synchronization
    /// status, we could be asking for more details about remote nodes' chain, or could be asking for blocks
    /// data from the lead node.
    ///
    pub fn tick<'n>(
        &mut self,
        store: &CS,
        nodes: &'n Nodes,
    ) -> Result<Vec<(&'n Node, FrameBuilder<chain_sync_request::Owned>)>, Error> {
        let node_id = self.node_id.clone();
        let config = self.config.clone();
        let mut requests_to_send = Vec::new();

        let (nb_nodes_metadata_sync, nb_nodes) = self.count_nodes_status(nodes);
        let majority_nodes_metadata_sync = nb_nodes_metadata_sync >= nb_nodes / 2;
        debug!(
            "Sync tick begins. current_status={:?} nb_nodes={} nb_nodes_metadata_sync={}",
            self.status, nb_nodes, nb_nodes_metadata_sync
        );

        // check if we need to gather data from a leader node
        if self.status != Status::Synchronized && majority_nodes_metadata_sync {
            if self.current_leader.is_none() {
                self.find_leader_node(store)?;
            }

            let im_leader = self
                .current_leader
                .as_ref()
                .map(|leader| leader == &node_id)
                .unwrap_or(false);
            if im_leader {
                debug!("I'm the leader. Switching status to synchronized");
                self.status = Status::Synchronized;
            } else if let Some(leader) = self.current_leader.clone() {
                debug!("Leader node is {}", leader);
                let leader_node_info = self.get_or_create_node_info_mut(&leader);

                if leader_node_info.chain_fully_downloaded() {
                    info!("Changing status to synchronized, as chain is full synchronized with leader");
                    self.status = Status::Synchronized
                } else {
                    if leader_node_info.last_common_block.as_ref().is_none() {
                        if let Some(last_block) = store.get_last_block()? {
                            error!("Leader node has no common block with us. Our last block is at offset {}", last_block.offset);
                            return Err(Error::Diverged(leader));
                        }
                    }

                    let (can_send_request, _last_request_timeout) =
                        leader_node_info.can_send_request(&config);
                    if can_send_request {
                        let leader_node = nodes.get(&node_id).ok_or_else(|| {
                            Error::Other(format!(
                                "Couldn't find leader node {} in nodes list",
                                node_id
                            ))
                        })?;

                        debug!("Initiating chain download with leader");
                        let request = Self::create_sync_request(
                            leader_node_info,
                            RequestedDetails::Blocks,
                            None,
                        )?;
                        leader_node_info.set_last_send(Instant::now());
                        requests_to_send.push((leader_node, request));
                        self.status = Status::Downloading;
                    }
                }
            } else {
                error!("Couldn't find any leader node !");
            }
        }

        // synchronize chain state with nodes
        for node in nodes.nodes().filter(|n| n.id != node_id) {
            let node_info = self.get_or_create_node_info_mut(node.id());

            let (can_send_request, _last_request_timeout) = node_info.can_send_request(&config);
            if can_send_request {
                let request =
                    Self::create_sync_request(node_info, RequestedDetails::Headers, None)?;
                node_info.set_last_send(Instant::now());
                requests_to_send.push((node, request));
            }
        }

        debug!("Sync tick ended. current_status={:?}", self.status);
        Ok(requests_to_send)
    }

    ///
    /// Creates a new sync request to be sent to a node, asking for headers or blocks. Headers are
    /// used remote node's chain metadata, while blocks are requested if we determined that a node
    /// is our leader.
    ///
    fn create_sync_request(
        node_info: &NodeSyncInfo,
        requested_details: RequestedDetails,
        to_offset: Option<BlockOffset>,
    ) -> Result<FrameBuilder<chain_sync_request::Owned>, Error> {
        let mut frame_builder = FrameBuilder::new();
        let mut request_builder: chain_sync_request::Builder = frame_builder.get_builder_typed();

        let from_offset = node_info
            .last_common_block
            .as_ref()
            .map(|b| {
                // if we requesting blocks, we want data from next offset to prevent getting data
                // for a block we have already have
                if requested_details == RequestedDetails::Headers {
                    b.offset
                } else {
                    b.next_offset()
                }
            })
            .unwrap_or(0);

        let to_offset = to_offset.unwrap_or_else(|| {
            node_info
                .last_known_block
                .as_ref()
                .map(|b| b.offset)
                .unwrap_or(0)
        });

        request_builder.set_from_offset(from_offset);
        request_builder.set_to_offset(to_offset);
        request_builder.set_requested_details(requested_details);

        debug!(
            "Sending sync_request to node={} from_offset={} to_offset={} requested_details={:?}",
            node_info.node_id,
            from_offset,
            to_offset,
            requested_details.to_u16(),
        );

        Ok(frame_builder)
    }

    ///
    /// Handles an incoming sync request. This request can be for headers, or could be for blocks.
    ///
    pub fn handle_sync_request(
        &mut self,
        from_node: &Node,
        store: &mut CS,
        request: OwnedTypedFrame<chain_sync_request::Owned>,
    ) -> Result<Option<FrameBuilder<chain_sync_response::Owned>>, Error> {
        let request_reader: chain_sync_request::Reader = request.get_typed_reader()?;
        let (from_offset, to_offset) = (
            request_reader.get_from_offset(),
            request_reader.get_to_offset(),
        );
        let requested_details = request_reader.get_requested_details()?;
        debug!(
            "Got request from node {} for offset from {} to offset {} requested_details={}",
            from_node.id,
            from_offset,
            to_offset,
            requested_details.to_u16()
        );

        let node_info = self.get_or_create_node_info_mut(&from_node.id());
        node_info.set_last_responded(Instant::now());

        if requested_details == chain_sync_request::RequestedDetails::Headers {
            let to_offset_opt = if to_offset != 0 {
                Some(to_offset)
            } else {
                None
            };

            let headers = chain_sample_block_headers(
                store,
                from_offset,
                to_offset_opt,
                self.config.headers_sync_begin_count,
                self.config.headers_sync_end_count,
                self.config.headers_sync_sampled_count,
            )?;

            Self::create_sync_response_for_headers(from_offset, to_offset, headers).map(Some)
        } else if requested_details == chain_sync_request::RequestedDetails::Blocks {
            let blocks_iter = store
                .block_iter(from_offset)?
                .filter(|b| to_offset == 0 || b.offset <= to_offset);
            Self::create_sync_response_for_blocks(&self.config, from_offset, to_offset, blocks_iter)
                .map(Some)
        } else {
            Err(Error::InvalidSyncRequest(format!(
                "Unsupported requested details: {:?}",
                requested_details.to_u16()
            )))
        }
    }

    ///
    /// Creates a response to a request for headers from a remote node.
    ///
    fn create_sync_response_for_headers(
        from_offset: chain::BlockOffset,
        to_offset: chain::BlockOffset,
        headers: Vec<BlockHeader>,
    ) -> Result<FrameBuilder<chain_sync_response::Owned>, Error> {
        let mut frame_builder = FrameBuilder::new();
        let mut response_builder: chain_sync_response::Builder = frame_builder.get_builder_typed();
        response_builder.set_from_offset(from_offset);
        response_builder.set_to_offset(to_offset);

        let mut headers_builder = response_builder.init_headers(headers.len() as u32);
        for (i, header) in headers.iter().enumerate() {
            header.copy_into_builder(&mut headers_builder.reborrow().get(i as u32));
        }

        debug!(
            "Sending {} header(s) from offset {:?} to offset {:?}",
            headers.len(),
            from_offset,
            to_offset,
        );

        Ok(frame_builder)
    }

    ///
    /// Creates a response to request for blocks data from a remote node.
    /// If we're asked for data, this means we're the lead.
    ///
    fn create_sync_response_for_blocks<'s, I: Iterator<Item = StoredBlock<'s>>>(
        config: &Config,
        from_offset: chain::BlockOffset,
        to_offset: chain::BlockOffset,
        blocks_iter: I,
    ) -> Result<FrameBuilder<chain_sync_response::Owned>, Error> {
        let mut frame_builder = FrameBuilder::new();
        let mut response_builder: chain_sync_response::Builder = frame_builder.get_builder_typed();
        response_builder.set_from_offset(from_offset);
        response_builder.set_to_offset(to_offset);

        // cumulates blocks' data until we reach max packet size
        let mut data_size = 0;
        let blocks = blocks_iter
            .take_while(|block| {
                data_size += block.block.frame_size() + block.signatures.frame_size();
                data_size < config.blocks_max_send_size
            })
            .collect::<Vec<_>>();
        let blocks_len = blocks.len() as u32;

        if blocks_len > 0 {
            let mut blocks_builder = response_builder.init_blocks(blocks_len);
            for i in 0..blocks_len {
                let block_and_signatures = vec![
                    blocks[i as usize].block.frame_data(),
                    blocks[i as usize].signatures.frame_data(),
                ]
                .concat();

                blocks_builder.reborrow().set(i, &block_and_signatures);
            }

            debug!(
                "Sending {} block(s) data with total size {} bytes from offset {:?} to offset {:?}",
                blocks.len(),
                data_size,
                blocks.first().map(|b| b.offset),
                blocks.last().map(|b| b.offset)
            );
        }

        Ok(frame_builder)
    }

    ///
    /// Handles a sync response from a node, that could contain either headers or blocks data.
    /// If it contains headers, we gather the knowledge about the remote node's chain metadata.
    /// If it contains data, it means that it comes from the leader node and we need to append data
    /// to our local chain.
    ///
    pub fn handle_sync_response(
        &mut self,
        from_node: &Node,
        store: &mut CS,
        response: OwnedTypedFrame<chain_sync_response::Owned>,
    ) -> Result<Option<FrameBuilder<chain_sync_request::Owned>>, Error> {
        let response_reader: chain_sync_response::Reader = response.get_typed_reader()?;
        let ret = if response_reader.has_blocks() {
            debug!("Got blocks response from node {}", from_node.id);
            self.handle_sync_response_blocks(from_node, store, response_reader)?
        } else if response_reader.has_headers() {
            debug!("Got headers response from node {}", from_node.id);
            self.handle_sync_response_headers(from_node, store, response_reader)?
        } else {
            warn!("Got a response without headers and blocks");
            None
        };

        // last responded is set at the end so that if we failed reading response, it's considered
        // as if we didn't receive anything (which will lead to timeout & retries)
        let node_info = self.get_or_create_node_info_mut(&from_node.id());
        node_info.set_last_responded(Instant::now());

        Ok(ret)
    }

    ///
    /// Manages headers response by comparing to local blocks and finding the common ancestor (if any)
    /// and the last block of the node against which we're syncing.
    ///
    /// If we didn't find the latest common ancestor, we reply with another request from the earliest
    /// common ancestor we could find so far.
    ///
    fn handle_sync_response_headers(
        &mut self,
        from_node: &Node,
        store: &mut CS,
        response_reader: chain_sync_response::Reader,
    ) -> Result<Option<FrameBuilder<chain_sync_request::Owned>>, Error> {
        let from_node_info = self.get_or_create_node_info_mut(&from_node.id());

        let headers_reader = response_reader.get_headers()?;
        let mut has_new_common_block = false;
        let mut first_non_common_block: Option<chain::BlockOffset> = None;
        for header in headers_reader.iter() {
            let header_reader: block_header::Reader = header;
            let offset = header_reader.get_offset();

            // if we haven't encountered a block we didn't have in common, we keep checking if we have
            // the block locally, and update the last_common_block
            if first_non_common_block.is_none() {
                if let Ok(local_block) = store.get_block(offset) {
                    let local_block_signature = local_block
                        .block
                        .signature_data()
                        .expect("A stored block didn't have a signature");
                    if header_reader.get_hash()? == local_block_signature {
                        let is_latest_common_offset = from_node_info
                            .last_common_block
                            .as_ref()
                            .map(|b| b.offset < offset)
                            .unwrap_or(true);
                        if is_latest_common_offset {
                            from_node_info.last_common_block =
                                Some(BlockHeader::from_block_header_reader(header_reader)?);
                            has_new_common_block = true;
                        }
                    } else {
                        first_non_common_block = Some(offset);
                    }
                } else {
                    first_non_common_block = Some(offset);
                }
            }

            // update last known block if it's higher than previously known one
            let is_latest_offset = from_node_info
                .last_known_block
                .as_ref()
                .map(|b| b.offset < offset)
                .unwrap_or(true);
            if is_latest_offset {
                from_node_info.last_known_block =
                    Some(BlockHeader::from_block_header_reader(header_reader)?);
            }
        }

        if has_new_common_block {
            let to_offset = first_non_common_block;
            debug!(
                "New common ancestor block: {:?} to {:?}. Asking for more headers.",
                from_node_info.last_common_block, first_non_common_block
            );

            let request =
                Self::create_sync_request(from_node_info, RequestedDetails::Headers, to_offset)?;
            return Ok(Some(request));
        } else {
            debug!(
                "Finished fetching metadata of node {}. last_known_block={:?}, last_common_ancestor={:?}",
                from_node_info.node_id,
                from_node_info.last_known_block,
                from_node_info.last_common_block
            );
            from_node_info.last_common_is_known = true;
            from_node_info.force_can_send_request = Some(true);
        }

        Ok(None)
    }

    ///
    /// Manages blocks (full data) response coming from the lead node, and appends them to our local chain.
    /// If there are still blocks after, we respond with a further request
    ///
    fn handle_sync_response_blocks(
        &mut self,
        from_node: &Node,
        store: &mut CS,
        response_reader: chain_sync_response::Reader,
    ) -> Result<Option<FrameBuilder<chain_sync_request::Owned>>, Error> {
        let lead_node_id = self.current_leader.as_ref().cloned();
        let from_node_info = self.get_or_create_node_info_mut(&from_node.id());

        let is_from_leader = lead_node_id
            .map(|lead_node_id| lead_node_id == from_node_info.node_id)
            .unwrap_or(false);
        if is_from_leader {
            // write incoming blocks
            let mut last_local_block: Option<BlockHeader> = store
                .get_last_block()?
                .map(BlockHeader::from_stored_block)
                .transpose()?;
            let blocks_reader = response_reader.get_blocks()?;
            for data_res in blocks_reader.iter() {
                // data contains both block + block_signatures
                let data = data_res?;

                // read block & signatures from data
                let block_frame = TypedSliceFrame::<block::Owned>::new(data)?;
                let block_reader: block::Reader = block_frame.get_typed_reader()?;
                if data.len() < block_frame.frame_size() {
                    return Err(Error::InvalidSyncResponse(format!(
                        "Got a block with unexpected size that couldn't have a signature:\
                         data_len={} block_frame_size={}",
                        data.len(),
                        block_frame.frame_size()
                    )));
                }
                let block_signatures_frame = TypedSliceFrame::<block_signatures::Owned>::new(
                    &data[block_frame.frame_size()..],
                )?;

                // make sure the block was expected in our chain, then add it
                let next_local_offset = last_local_block
                    .as_ref()
                    .map(|b| b.next_offset())
                    .unwrap_or(0);
                if block_reader.get_offset() == next_local_offset {
                    store.write_block(&block_frame, &block_signatures_frame)?;
                    let new_block = store.get_block(next_local_offset)?;
                    let new_block_header = BlockHeader::from_stored_block(new_block)?;
                    last_local_block = Some(new_block_header);
                } else {
                    return Err(Error::InvalidSyncResponse(format!(
                        "Got a block with data at an invalid offset. \
                         expected_offset={} block_offset={}",
                        next_local_offset,
                        block_reader.get_offset()
                    )));
                }
            }
            from_node_info.last_common_block = last_local_block;

            // check if we're done
            if from_node_info.chain_fully_downloaded() {
                info!("Finished downloading chain from leader node !");
                self.status = Status::Synchronized;
                Ok(None)
            } else {
                let request =
                    Self::create_sync_request(from_node_info, RequestedDetails::Blocks, None)?;
                from_node_info.set_last_send(Instant::now());
                Ok(Some(request))
            }
        } else {
            warn!("Got data from a non-lead node {:?}", from_node_info.node_id);
            Ok(None)
        }
    }

    fn get_or_create_node_info_mut(&mut self, node_id: &str) -> &mut NodeSyncInfo {
        if self.nodes_info.contains_key(node_id) {
            return self.nodes_info.get_mut(node_id).unwrap();
        }

        self.nodes_info
            .entry(node_id.to_string())
            .or_insert_with(move || NodeSyncInfo::new(node_id.to_string()))
    }

    fn count_nodes_status(&self, nodes: &Nodes) -> (u16, u16) {
        let mut nodes_total = 0;
        let mut nodes_metadata_sync = 0;
        for node in nodes.nodes() {
            nodes_total += 1;

            if node.id == self.node_id {
                continue;
            }

            if let Some(node_info) = self.nodes_info.get(&node.id) {
                if node_info.chain_metadata_status() == NodeMetadataStatus::Synchronized {
                    nodes_metadata_sync += 1;
                }
            }
        }

        (nodes_metadata_sync, nodes_total)
    }

    fn find_leader_node(&mut self, store: &CS) -> Result<(), Error> {
        let maybe_leader = self
            .nodes_info
            .values()
            .filter(|info| info.last_known_block.is_some())
            .map(|info| {
                let last_offset = info
                    .last_known_block
                    .as_ref()
                    .map(|b| b.offset)
                    .expect("Node should have had a last_known_block");
                (info, last_offset)
            })
            .max_by(|(_node_a, offset_a), (_node_b, offset_b)| offset_a.cmp(offset_b));

        let last_local_block = store.get_last_block()?;
        self.current_leader = match (maybe_leader, &last_local_block) {
            (Some((_node_info, node_offset)), Some(last_last_block))
                if last_last_block.offset > node_offset =>
            {
                Some(self.node_id.clone())
            }
            (Some((node_info, _)), _) => Some(node_info.node_id.clone()),
            _ => None,
        };

        Ok(())
    }
}

///
/// Synchronization information about a remote node
///
struct NodeSyncInfo {
    node_id: NodeID,

    last_common_block: Option<BlockHeader>,
    last_common_is_known: bool,
    last_known_block: Option<BlockHeader>,

    last_request_send: Option<Instant>,
    last_response_receive: Option<Instant>,
    nb_response_failure: usize,

    // next can_send_request() will return true
    force_can_send_request: Option<bool>,
}

impl NodeSyncInfo {
    fn new(node_id: NodeID) -> NodeSyncInfo {
        NodeSyncInfo {
            node_id,

            last_common_block: None,
            last_common_is_known: false,
            last_known_block: None,
            force_can_send_request: None,

            last_request_send: None,
            last_response_receive: None,
            nb_response_failure: 0,
        }
    }

    fn chain_metadata_status(&self) -> NodeMetadataStatus {
        if self.last_common_is_known {
            NodeMetadataStatus::Synchronized
        } else {
            NodeMetadataStatus::Unknown
        }
    }

    fn chain_fully_downloaded(&self) -> bool {
        let last_known_offset = self.last_known_block.as_ref().map(|b| b.offset);
        let last_common_offset = self.last_common_block.as_ref().map(|b| b.offset);
        self.last_known_block.is_some() && last_known_offset == last_common_offset
    }

    fn set_last_send(&mut self, time: Instant) {
        self.last_request_send = Some(time);
    }

    fn set_last_responded(&mut self, time: Instant) {
        self.last_response_receive = Some(time);
        self.nb_response_failure = 0;
    }

    fn can_send_request(&mut self, config: &Config) -> (bool, bool) {
        if let Some(_force) = self.force_can_send_request.take() {
            return (true, false);
        }

        let request_timeout_duration = self.request_timeout_duration(config);
        let should_send_request = self
            .last_request_send
            .map(|i| i.elapsed() > request_timeout_duration)
            .unwrap_or(true);

        let mut last_request_timeout = false;
        if should_send_request {
            if !self.has_responded_last_request() {
                self.nb_response_failure += 1;
                last_request_timeout = true;
            }

            (true, last_request_timeout)
        } else {
            (false, last_request_timeout)
        }
    }

    fn request_timeout_duration(&self, config: &Config) -> Duration {
        config.base_request_timeout
    }

    fn has_responded_last_request(&self) -> bool {
        match (self.last_request_send, self.last_response_receive) {
            (Some(send), Some(resp)) if resp > send => true,
            _ => false,
        }
    }
}

#[derive(Debug, PartialEq, Clone)]
enum NodeMetadataStatus {
    Unknown,
    Synchronized,
}

///
/// Abstracts block's header coming from local store or remote node
///
#[derive(Debug)]
struct BlockHeader {
    offset: chain::BlockOffset,
    depth: chain::BlockDepth,
    hash: Vec<u8>,
    previous_offset: chain::BlockOffset,
    previous_hash: Vec<u8>,

    block_size: chain::BlockOffset,
    signatures_size: chain::BlockSignaturesSize,
}

impl BlockHeader {
    fn from_stored_block(stored_block: StoredBlock) -> Result<BlockHeader, Error> {
        let block_reader: block::Reader = stored_block.block.get_typed_reader()?;
        let block_signature = stored_block
            .block
            .signature_data()
            .expect("A stored block didn't signature data");

        Ok(BlockHeader {
            offset: stored_block.offset,
            depth: block_reader.get_depth(),
            hash: block_signature.to_vec(),
            previous_offset: block_reader.get_previous_offset(),
            previous_hash: block_reader.get_previous_hash()?.to_vec(),
            block_size: stored_block.block.frame_size() as chain::BlockOffset,
            signatures_size: stored_block.signatures.frame_size() as chain::BlockSignaturesSize,
        })
    }

    fn from_block_header_reader(
        block_header_reader: block_header::Reader,
    ) -> Result<BlockHeader, Error> {
        Ok(BlockHeader {
            offset: block_header_reader.get_offset(),
            depth: block_header_reader.get_depth(),
            hash: block_header_reader.get_hash()?.to_vec(),
            previous_offset: block_header_reader.get_previous_offset(),
            previous_hash: block_header_reader.get_previous_hash()?.to_vec(),
            block_size: block_header_reader.get_block_size(),
            signatures_size: block_header_reader.get_signatures_size(),
        })
    }

    #[inline]
    fn next_offset(&self) -> chain::BlockOffset {
        self.offset + self.block_size + chain::BlockOffset::from(self.signatures_size)
    }

    fn copy_into_builder(&self, builder: &mut block_header::Builder) {
        builder.set_offset(self.offset);
        builder.set_depth(self.depth);
        builder.set_hash(&self.hash);
        builder.set_previous_offset(self.previous_offset);
        builder.set_previous_hash(&self.previous_hash);
        builder.set_block_size(self.block_size);
        builder.set_signatures_size(self.signatures_size);
    }
}

///
/// Synchronizer's error
///
#[derive(Debug, Fail)]
enum Error {
    #[fail(display = "Error in chain store: {:?}", _0)]
    Store(#[fail(cause)] chain::Error),
    #[fail(display = "Error in framing serialization: {:?}", _0)]
    Framing(#[fail(cause)] framed::Error),
    #[fail(display = "Error in capnp serialization: kind={:?} msg={}", _0, _1)]
    Serialization(capnp::ErrorKind, String),
    #[fail(display = "Field is not in capnp schema: code={}", _0)]
    SerializationNotInSchema(u16),
    #[fail(display = "Got an invalid sync request: {}", _0)]
    InvalidSyncRequest(String),
    #[fail(display = "Got an invalid sync response: {}", _0)]
    InvalidSyncResponse(String),
    #[fail(display = "Our local chain has diverged from leader node: {}", _0)]
    Diverged(String),
    #[fail(display = "Got an error: {}", _0)]
    Other(String),
}

impl From<capnp::Error> for Error {
    fn from(err: capnp::Error) -> Self {
        Error::Serialization(err.kind, err.description)
    }
}

impl From<framed::Error> for Error {
    fn from(err: framed::Error) -> Self {
        Error::Framing(err)
    }
}

impl From<chain::Error> for Error {
    fn from(err: chain::Error) -> Self {
        Error::Store(err)
    }
}

impl From<capnp::NotInSchema> for Error {
    fn from(err: capnp::NotInSchema) -> Self {
        Error::SerializationNotInSchema(err.0)
    }
}

///
/// Samples the local chain and returns a collection of `BlockHeader` at different position in the asked range.
///
/// `from_offset` and `to_offset` are best efforts and fallback to begin/end of chain if they don't exist.
/// `begin_count` and `end_count` are number of headers to include without sampling from beginning and end of range.
/// `sampled_count` is the approximate number of headers to return, excluding the `begin_count` and `end_count`
///
fn chain_sample_block_headers<CS: chain::Store>(
    store: &CS,
    from_offset: chain::BlockOffset,
    to_offset: Option<chain::BlockOffset>,
    begin_count: chain::BlockOffset,
    end_count: chain::BlockOffset,
    sampled_count: chain::BlockOffset,
) -> Result<Vec<BlockHeader>, Error> {
    let mut headers = Vec::new();

    let segments_range = store.available_segments();
    if segments_range.is_empty() {
        return Ok(headers);
    }

    let last_block = match to_offset {
        Some(offset) => store.get_block(offset).map(Some).or_else(|_| {
            warn!(
                "Given to offset {} didn't exist. Falling back to last block of chain",
                offset
            );
            store.get_last_block()
        }),
        None => store.get_last_block(),
    }?
    .ok_or_else(|| Error::Other("Expected a last block since ranges were not empty".to_string()))?;

    let last_block_reader: block::Reader = last_block.block.get_typed_reader()?;
    let last_block_depth = last_block_reader.get_depth();

    let mut blocks_iter = store
        .block_iter(from_offset)
        .or_else(|_| store.block_iter(0))?
        .peekable();

    let first_block = blocks_iter.peek().ok_or_else(|| {
        Error::Other("Expected a first block since ranges were not empty".to_string())
    })?;
    let first_block_reader: block::Reader = first_block.block.get_typed_reader()?;
    let first_block_depth = first_block_reader.get_depth();

    let range_blocks_count = last_block_depth - first_block_depth;
    let range_blocks_skip = (range_blocks_count / sampled_count).max(1);

    // from which block do we include all headers so that we always include last `end_count` blocks
    let range_blocks_lasts = range_blocks_count
        .checked_sub(end_count)
        .unwrap_or(range_blocks_count);

    for (blocks_count, current_block) in blocks_iter
        .enumerate()
        .take(range_blocks_count as usize + 1)
    {
        // we always include headers if the block is within the first `begin_count` or in the last `end_count`
        // otherwise, we include if it falls within sampling condition
        let blocks_count = blocks_count as chain::BlockOffset;
        if blocks_count < begin_count
            || blocks_count > range_blocks_lasts
            || blocks_count % range_blocks_skip == 0
        {
            let block_header = BlockHeader::from_stored_block(current_block)?;
            headers.push(block_header);
        }
    }

    Ok(headers)
}

#[cfg(test)]
mod tests {
    use tempdir::TempDir;

    use chain::directory::{DirectoryConfig, DirectoryStore};
    use exocore_common::serialization::framed::MultihashFrameSigner;

    use super::*;
    use crate::engine::chain_sync::Status::Synchronized;

    #[test]
    fn test_handle_sync_response_blocks() {
        crate::utils::setup_logging();

        let mut nodes = Nodes::new();
        nodes.add(Node::new("node1".to_string()));
        nodes.add(Node::new("node2".to_string()));

        let mut test_node1 = TestNode::new("node1".to_string());
        test_node1.generate_chain(10, 1234);

        let mut test_node2 = TestNode::new("node2".to_string());
        test_node2.generate_chain(100, 1234);

        {
            test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
            test_node1
                .synchronizer
                .tick(&test_node1.chain_store, &nodes)
                .unwrap();
            assert_eq!(test_node1.synchronizer.status, Status::Downloading);
            assert_eq!(
                test_node1.synchronizer.current_leader,
                Some("node2".to_string())
            );
        }

        // response from non-leader should just not reply
        let mut response = FrameBuilder::new();
        let mut response_builder = response.get_builder_typed();
        let mut response_frame = response.as_owned_unsigned_framed().unwrap();
        let request = test_node1
            .synchronizer
            .handle_sync_response(
                nodes.get("node1").unwrap(),
                &mut test_node1.chain_store,
                response_frame,
            )
            .unwrap();
        assert!(request.is_none());

        // response from leader with blocks that aren't next should fail
        let blocks_iter = test_node2.chain_store.block_iter(0).unwrap();
        let response = Synchronizer::<DirectoryStore>::create_sync_response_for_blocks(
            &test_node1.synchronizer.config,
            10,
            0,
            blocks_iter,
        )
        .unwrap();
        let response_frame = response.as_owned_unsigned_framed().unwrap();
        let request = test_node1.synchronizer.handle_sync_response(
            nodes.get("node2").unwrap(),
            &mut test_node1.chain_store,
            response_frame,
        );
        assert!(request.is_err());

        // response from leader with blocks at right position should suceed and append
        let blocks_iter = test_node2.chain_store.block_iter(0).unwrap().skip(10); // skip 10 will go to 10th block
        let response = Synchronizer::<DirectoryStore>::create_sync_response_for_blocks(
            &test_node1.synchronizer.config,
            10,
            0,
            blocks_iter,
        )
        .unwrap();
        let response_frame = response.as_owned_unsigned_framed().unwrap();
        let request = test_node1.synchronizer.handle_sync_response(
            nodes.get("node2").unwrap(),
            &mut test_node1.chain_store,
            response_frame,
        );
        assert!(request.is_ok());
    }

    #[test]
    fn sync_empty_node1_to_full_node2() {
        let mut nodes = Nodes::new();
        nodes.add(Node::new("node1".to_string()));
        nodes.add(Node::new("node2".to_string()));
        let node1 = nodes.get("node1").unwrap();
        let node2 = nodes.get("node2").unwrap();

        let mut test_node1 = TestNode::new(node1.id.clone());
        let mut test_node2 = TestNode::new(node2.id.clone());
        test_node2.generate_chain(100, 3424);

        test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
        {
            let node1_node2_info = test_node1.synchronizer.nodes_info.get("node2").unwrap();
            assert_eq!(
                node1_node2_info.chain_metadata_status(),
                NodeMetadataStatus::Synchronized
            );
            assert_eq!(
                node1_node2_info.last_common_block.as_ref().map(|b| b.depth),
                None
            );
            assert_eq!(
                node1_node2_info.last_known_block.as_ref().map(|b| b.depth),
                Some(99)
            );
        }

        // this will sync blocks & mark as synchronized
        test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
        assert_eq!(test_node1.synchronizer.status, Status::Synchronized);
        assert_eq!(
            test_node1.synchronizer.current_leader,
            Some("node2".to_string())
        );

        // force status back to downloading to check if tick will turn back to synchronized
        test_node1.synchronizer.status = Status::Downloading;
        test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
        assert_eq!(test_node1.synchronizer.status, Status::Synchronized);

        test_nodes_expect_chain_equals(test_node1, test_node2);
    }

    #[test]
    fn sync_full_node1_to_empty_node2() {
        let mut nodes = Nodes::new();
        nodes.add(Node::new("node1".to_string()));
        nodes.add(Node::new("node2".to_string()));
        let node1 = nodes.get("node1").unwrap();
        let node2 = nodes.get("node2").unwrap();

        let mut test_node1 = TestNode::new(node1.id.clone());
        let mut test_node2 = TestNode::new(node2.id.clone());
        test_node1.generate_chain(100, 3424);

        // running sync twice will yield to nothing as node2 is empty
        for i in 0..2 {
            test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
            let node1_node2_info = test_node1.synchronizer.nodes_info.get("node2").unwrap();
            assert_eq!(
                node1_node2_info.chain_metadata_status(),
                NodeMetadataStatus::Synchronized
            );
            assert_eq!(
                node1_node2_info.last_common_block.as_ref().map(|b| b.depth),
                None
            );
            assert_eq!(
                node1_node2_info.last_known_block.as_ref().map(|b| b.depth),
                None
            );
        }

        // unknown, as quorum is not met
        assert_eq!(test_node1.synchronizer.status, Status::Unknown);
    }

    #[test]
    fn sync_full_node1_to_half_node2() {
        let mut nodes = Nodes::new();
        nodes.add(Node::new("node1".to_string()));
        nodes.add(Node::new("node2".to_string()));
        let node1 = nodes.get("node1").unwrap();
        let node2 = nodes.get("node2").unwrap();

        let mut test_node1 = TestNode::new(node1.id.clone());
        let mut test_node2 = TestNode::new(node2.id.clone());
        test_node1.generate_chain(100, 3424);
        test_node2.generate_chain(50, 3424);

        // running sync twice will yield to nothing as node1 is leader
        for i in 0..2 {
            test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
            let node1_node2_info = test_node1.synchronizer.nodes_info.get("node2").unwrap();
            assert_eq!(
                node1_node2_info.chain_metadata_status(),
                NodeMetadataStatus::Synchronized
            );
            assert_eq!(
                node1_node2_info.last_common_block.as_ref().map(|b| b.depth),
                Some(49)
            );
            assert_eq!(
                node1_node2_info.last_known_block.as_ref().map(|b| b.depth),
                Some(49)
            );
        }

        // we're leader and synchronized because of it
        assert_eq!(
            test_node1.synchronizer.current_leader,
            Some("node1".to_string())
        );
        assert_eq!(test_node1.synchronizer.status, Status::Synchronized);
    }

    #[test]
    fn sync_half_node1_to_full_node2() {
        let mut nodes = Nodes::new();
        nodes.add(Node::new("node1".to_string()));
        nodes.add(Node::new("node2".to_string()));
        let node1 = nodes.get("node1").unwrap();
        let node2 = nodes.get("node2").unwrap();

        let mut test_node1 = TestNode::new(node1.id.clone());
        test_node1.generate_chain(50, 3424);

        let mut test_node2 = TestNode::new(node2.id.clone());
        test_node2.generate_chain(100, 3424);

        test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
        {
            let node1_node2_info = test_node1.synchronizer.nodes_info.get("node2").unwrap();
            assert_eq!(
                node1_node2_info.chain_metadata_status(),
                NodeMetadataStatus::Synchronized
            );
            assert_eq!(
                node1_node2_info.last_common_block.as_ref().map(|b| b.depth),
                Some(49)
            );
            assert_eq!(
                node1_node2_info.last_known_block.as_ref().map(|b| b.depth),
                Some(99)
            );
        }

        // this will sync blocks & mark as synchronized
        test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();

        // node2 is leader
        assert_eq!(
            test_node1.synchronizer.current_leader,
            Some("node2".to_string())
        );
        assert_eq!(test_node1.synchronizer.status, Status::Synchronized);

        test_nodes_expect_chain_equals(test_node1, test_node2);
    }

    #[test]
    fn sync_divergent_node1_to_full_node2() {
        let mut nodes = Nodes::new();
        nodes.add(Node::new("node1".to_string()));
        nodes.add(Node::new("node2".to_string()));
        let node1 = nodes.get("node1").unwrap();
        let node2 = nodes.get("node2").unwrap();

        let mut test_node1 = TestNode::new(node1.id.clone());
        test_node1.generate_chain(100, 1234);

        let mut test_node2 = TestNode::new(node2.id.clone());
        test_node2.generate_chain(100, 9876);

        test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).unwrap();
        {
            let node1_node2_info = test_node1.synchronizer.nodes_info.get("node2").unwrap();
            assert_eq!(
                node1_node2_info.chain_metadata_status(),
                NodeMetadataStatus::Synchronized
            );
            assert_eq!(
                node1_node2_info.last_common_block.as_ref().map(|b| b.depth),
                None,
            );
            assert_eq!(
                node1_node2_info.last_known_block.as_ref().map(|b| b.depth),
                Some(99),
            );
        }

        match test_nodes_run_sync(&mut nodes, &mut test_node1, &mut test_node2).err() {
            Some(Error::Diverged(_)) => {}
            other => panic!("Expected a diverged error, got {:?}", other),
        }

        // still unknown since we don't have a clear leader, as we've diverged from it
        assert_eq!(test_node1.synchronizer.status, Status::Unknown);
    }

    #[test]
    fn test_chain_sample_block_headers() {
        let mut test_node = TestNode::new("node1".to_string());
        test_node.generate_chain(100, 3424);
        let offsets: Vec<chain::BlockOffset> = test_node
            .chain_store
            .block_iter(0)
            .unwrap()
            .map(|b| b.offset)
            .collect();

        let headers =
            chain_sample_block_headers(&test_node.chain_store, 0, None, 2, 2, 10).unwrap();
        assert_eq!(
            headers.iter().map(|b| b.depth).collect::<Vec<_>>(),
            vec![0, 1, 9, 18, 27, 36, 45, 54, 63, 72, 81, 90, 98, 99]
        );

        let headers = chain_sample_block_headers(&test_node.chain_store, 0, None, 0, 0, 1).unwrap();
        assert_eq!(
            headers.iter().map(|b| b.depth).collect::<Vec<_>>(),
            vec![0, 99]
        );

        let headers =
            chain_sample_block_headers(&test_node.chain_store, offsets[10], None, 5, 5, 10)
                .unwrap();
        assert_eq!(
            headers.iter().map(|b| b.depth).collect::<Vec<_>>(),
            vec![10, 11, 12, 13, 14, 18, 26, 34, 42, 50, 58, 66, 74, 82, 90, 95, 96, 97, 98, 99]
        );

        let headers = chain_sample_block_headers(
            &test_node.chain_store,
            offsets[10],
            Some(offsets[50]),
            2,
            2,
            5,
        )
        .unwrap();
        assert_eq!(
            headers.iter().map(|b| b.depth).collect::<Vec<_>>(),
            vec![10, 11, 18, 26, 34, 42, 49, 50]
        );
    }

    struct TestNode {
        node_id: NodeID,
        tempdir: TempDir,
        chain_store: DirectoryStore,
        synchronizer: Synchronizer<DirectoryStore>,
    }

    impl TestNode {
        fn new(node_id: NodeID) -> TestNode {
            let tempdir = TempDir::new("chain_sync").unwrap();
            let chain_config = DirectoryConfig {
                segment_max_size: 3000,
                ..DirectoryConfig::default()
            };
            let chain_store = DirectoryStore::create(chain_config, tempdir.as_ref()).unwrap();

            let synchronizer_config = Config::default();
            let synchronizer = Synchronizer::new(node_id.clone(), synchronizer_config);

            TestNode {
                node_id,
                tempdir,
                chain_store,
                synchronizer,
            }
        }

        fn generate_chain(&mut self, count: usize, seed: u64) {
            let mut offsets = Vec::new();
            let mut next_offset = 0;

            for i in 0..count {
                offsets.push(next_offset);

                let previous_block = if i != 0 {
                    Some(
                        self.chain_store
                            .get_block_from_next_offset(next_offset)
                            .unwrap(),
                    )
                } else {
                    None
                };

                let prev_block_msg = previous_block.map(|b| b.block);
                let block = create_block(next_offset, i as u64, prev_block_msg, seed);
                let block_signature = create_block_sigs();
                next_offset = self
                    .chain_store
                    .write_block(&block, &block_signature)
                    .unwrap();
            }
        }
    }

    fn test_nodes_run_sync(
        nodes: &mut Nodes,
        test_node1: &mut TestNode,
        test_node2: &mut TestNode,
    ) -> Result<(usize, usize), Error> {
        let mut count_1_to_2 = 0;
        let mut count_2_to_1 = 0;

        let init_requests = test_node1
            .synchronizer
            .tick(&test_node1.chain_store, &nodes)?;
        if init_requests.is_empty() {
            return Ok((count_1_to_2, count_2_to_1));
        }

        let mut request = Some(init_requests[0].1.as_owned_unsigned_framed()?);
        loop {
            count_1_to_2 += 1;
            let node1 = nodes.get(&test_node1.node_id).unwrap();
            let response = test_node2.synchronizer.handle_sync_request(
                node1,
                &mut test_node2.chain_store,
                request.take().unwrap(),
            )?;

            if let Some(response) = response {
                count_2_to_1 += 1;

                let response = response.as_owned_unsigned_framed()?;
                let node2 = nodes.get(&test_node2.node_id).unwrap();
                let next_request = test_node1.synchronizer.handle_sync_response(
                    node2,
                    &mut test_node1.chain_store,
                    response,
                )?;

                if let Some(next_request) = next_request {
                    request = Some(next_request.as_owned_unsigned_framed()?);
                } else {
                    break;
                }
            } else {
                break;
            }
        }

        Ok((count_1_to_2, count_2_to_1))
    }

    fn test_nodes_expect_chain_equals(test_node1: TestNode, test_node2: TestNode) {
        let node1_last_block = test_node1
            .chain_store
            .get_last_block()
            .unwrap()
            .expect("Node 1 didn't have any data");
        let node2_last_block = test_node2
            .chain_store
            .get_last_block()
            .unwrap()
            .expect("Node 2 didn't have any data");
        assert_eq!(node1_last_block.offset, node2_last_block.offset);
        assert_eq!(
            node1_last_block.block.frame_data(),
            node2_last_block.block.frame_data()
        );
        assert_eq!(
            node1_last_block.signatures.frame_data(),
            node2_last_block.signatures.frame_data()
        );
    }

    fn create_block<B: TypedFrame<block::Owned>>(
        offset: u64,
        depth: u64,
        previous_block: Option<B>,
        seed: u64,
    ) -> OwnedTypedFrame<block::Owned> {
        let mut msg_builder = framed::FrameBuilder::<block::Owned>::new();

        {
            let mut block_builder: block::Builder = msg_builder.get_builder_typed();
            block_builder.set_offset(offset);
            block_builder.set_depth(depth);
            block_builder.set_source_node_id(&format!("seed={}", seed));

            if let Some(previous_block) = previous_block {
                let previous_block_reader: block::Reader =
                    previous_block.get_typed_reader().unwrap();
                block_builder.set_previous_offset(previous_block_reader.get_offset());
                block_builder.set_previous_hash(previous_block.signature_data().unwrap());
            }
        }

        let signer = MultihashFrameSigner::new_sha3256();
        msg_builder.as_owned_framed(signer).unwrap()
    }

    fn create_block_sigs() -> OwnedTypedFrame<block_signatures::Owned> {
        let msg_builder = framed::FrameBuilder::<block_signatures::Owned>::new();

        let signer = MultihashFrameSigner::new_sha3256();
        msg_builder.as_owned_framed(signer).unwrap()
    }
}
