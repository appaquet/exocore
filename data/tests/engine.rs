#[macro_use]
extern crate log;

use futures::prelude::*;
use tempdir;
use tokio::runtime::Runtime;

use exocore_common::node::{Node, Nodes};
use exocore_common::serialization::framed::TypedFrame;
use exocore_common::time::Clock;
use exocore_data::chain::ChainStore;
use exocore_data::{
    DirectoryChainStore, DirectoryChainStoreConfig, Engine, EngineConfig, MemoryPendingStore,
    MockTransportHub,
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
    let transport = transport_hub.get_transport(nodes.get("node1").unwrap().clone());

    let chain_config = DirectoryChainStoreConfig::default();
    let mut chain = DirectoryChainStore::create(chain_config, data_dir.as_ref())?;
    chain.write_block(&exocore_data::chain::BlockOwned::new_genesis(
        &nodes, &node1,
    )?)?;

    let pending = MemoryPendingStore::new();
    let clock = Clock::new();

    let engine_config = EngineConfig {
        manager_timer_interval: Duration::from_millis(100),
        ..EngineConfig::default()
    };
    let mut engine = Engine::new(
        engine_config,
        node1.id().to_string(),
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

    let _op1 = engine_handle.write_entry(b"i love jello")?;
    let op2 = engine_handle.write_entry(b"i love jello")?;
    let _op3 = engine_handle.write_entry(b"i love jello")?;
    let _op4 = engine_handle.write_entry(b"i love jello")?;

    // TODO: Should wait for events
    std::thread::sleep(Duration::from_millis(1000));

    let pending_operations = engine_handle.get_pending_operations(..)?;
    info!("Got {} pending op", pending_operations.len());

    let segments = engine_handle.get_chain_segments()?;
    info!("Available segments: {:?}", segments);

    let entry = engine_handle.get_chain_entry(332, op2).unwrap();
    info!(
        "Chain op: {:?}",
        String::from_utf8_lossy(entry.operation_frame.frame_data())
    );

    Ok(())
}
