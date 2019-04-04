#[macro_use]
extern crate log;

use futures::prelude::*;
use tempdir;
use tokio::runtime::Runtime;

use exocore_common::node::{Node, Nodes};
use exocore_common::serialization::framed::TypedFrame;
use exocore_common::time::Clock;
use exocore_data::chain::Store;
use exocore_data::{
    ChainDirectoryConfig, ChainDirectoryStore, Engine, EngineConfig, MemoryPendingStore,
    MockTransportHub, NewEntry,
};
use std::time::Duration;

#[test]
fn test_engine_integration_single_node() -> Result<(), failure::Error> {
    //exocore_common::utils::setup_logging();

    let data_dir = tempdir::TempDir::new("engine_tests")?;
    let mut rt = Runtime::new()?;

    let mut nodes = Nodes::new();
    let node1 = Node::new("node1".to_string());
    nodes.add(node1.clone());

    let transport_hub = MockTransportHub::default();

    // TODO: Doesn't make sense to clone a Node
    let transport = transport_hub.get_transport(nodes.get("node1").unwrap().clone());
    let engine_config = EngineConfig::default();
    let mut chain =
        ChainDirectoryStore::create(ChainDirectoryConfig::default(), data_dir.as_ref())?;

    let genesis_block = exocore_data::chain::BlockOwned::new_genesis(&nodes, &node1)?;
    chain.write_block(&genesis_block)?;

    let pending = MemoryPendingStore::new();

    let clock = Clock::new();

    let mut engine = Engine::new(
        engine_config,
        "node1".to_string(),
        clock,
        transport,
        chain,
        pending,
        nodes,
    );

    let engine_handle = engine.get_handle();

    rt.spawn(engine.map_err(|err| error!("Got an error in engine: {:?}", err)));
    // TODO: Find another way... engine may not have been started yet.

    std::thread::sleep(Duration::from_millis(300));

    engine_handle.write_entry(NewEntry::new_cell_data(1, b"i love jello".to_vec()))?;
    engine_handle.write_entry(NewEntry::new_cell_data(2, b"i love jello".to_vec()))?;
    engine_handle.write_entry(NewEntry::new_cell_data(3, b"i love jello".to_vec()))?;
    engine_handle.write_entry(NewEntry::new_cell_data(4, b"i love jello".to_vec()))?;

    std::thread::sleep(Duration::from_millis(5000));

    let pending_operations = engine_handle.get_pending_operations(..)?;
    info!("Got {} pending op", pending_operations.len());

    let segments = engine_handle.get_chain_available_segments()?;
    info!("Available segments: {:?}", segments);

    let entry = engine_handle.get_chain_entry(332, 2).unwrap();
    info!(
        "Chain op: {:?}",
        String::from_utf8_lossy(entry.operation_frame.frame_data())
    );

    Ok(())
}
