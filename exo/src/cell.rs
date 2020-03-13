use crate::options;
use exocore_core::cell::NodeConfig;
use exocore_core::protos::generated::data_chain_capnp::block_header;
use exocore_data::block::Block;
use exocore_data::chain::ChainStore;
use exocore_data::{DirectoryChainStore, DirectoryChainStoreConfig};

pub fn create_genesis_block(
    _opt: &options::Options,
    cell_opts: &options::CellOptions,
) -> Result<(), failure::Error> {
    let config = NodeConfig::from_yaml_file(&cell_opts.config)?;
    let local_node = config.create_local_node()?;

    let cell_config = config
        .cells
        .iter()
        .find(|config| config.public_key == cell_opts.public_key)
        .expect("Couldn't find cell with given public key");

    let (full_cell, _cell) = cell_config.create_cell(&local_node)?;
    let full_cell = full_cell.expect("Cannot create genesis block on a non-full cell");

    let mut chain_dir = cell_config.data_directory.clone();
    chain_dir.push("chain");
    std::fs::create_dir_all(&chain_dir)?;

    let mut chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;
    if chain_store.get_last_block()?.is_some() {
        panic!("Chain is already initialized");
    }

    let genesis_block = exocore_data::block::BlockOwned::new_genesis(&full_cell)?;
    chain_store.write_block(&genesis_block)?;

    Ok(())
}

pub fn check_chain(
    _opt: &options::Options,
    cell_opts: &options::CellOptions,
) -> Result<(), failure::Error> {
    let config = NodeConfig::from_yaml_file(&cell_opts.config)?;
    let cell_config = config
        .cells
        .iter()
        .find(|config| config.public_key == cell_opts.public_key)
        .expect("Couldn't find cell with given public key");

    let mut chain_dir = cell_config.data_directory.clone();
    chain_dir.push("chain");
    std::fs::create_dir_all(&chain_dir)?;

    let chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;

    for block in chain_store.blocks_iter(0)? {
        if let Err(err) = block.validate() {
            let block_header_reader = block.header().get_reader();
            let block_height = block_header_reader
                .map(block_header::Reader::get_height)
                .ok();

            error!(
                "Block at offset={} height={:?} is invalid: {}",
                block.offset(),
                block_height,
                err
            );
            return Ok(());
        }
    }

    Ok(())
}
