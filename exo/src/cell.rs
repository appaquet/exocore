use crate::options;
use exocore_chain::block::Block;
use exocore_chain::chain::ChainStore;
use exocore_chain::{DirectoryChainStore, DirectoryChainStoreConfig};
use exocore_core::cell::Cell;
use exocore_core::{
    framing::{FrameBuilder, FrameReader, SizedFrameBuilder},
    protos::generated::data_chain_capnp::block_header,
};
use std::io::Write;

pub fn create_genesis_block(
    _exo_opts: &options::ExoOptions,
    cell_opts: &options::CellOptions,
) -> anyhow::Result<()> {
    let config = exocore_core::cell::node_config_from_yaml_file(&cell_opts.config)?;
    let (either_cells, _local_node) = Cell::new_from_local_node_config(config)?;
    let full_cell = either_cells
        .into_iter()
        .find(|c| c.cell().public_key().encode_base58_string() == cell_opts.public_key)
        .expect("Couldn't find cell with given public key")
        .unwrap_full();

    let chain_dir = full_cell
        .chain_directory()
        .expect("Cell doesn't have a path configured");
    std::fs::create_dir_all(&chain_dir)?;

    let mut chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;
    if chain_store.get_last_block()?.is_some() {
        panic!("Chain is already initialized");
    }

    let genesis_block = exocore_chain::block::BlockOwned::new_genesis(&full_cell)?;
    chain_store.write_block(&genesis_block)?;

    Ok(())
}

pub fn check_chain(
    _exo_opts: &options::ExoOptions,
    cell_opts: &options::CellOptions,
) -> anyhow::Result<()> {
    let config = exocore_core::cell::node_config_from_yaml_file(&cell_opts.config)?;
    let (either_cells, _local_node) = Cell::new_from_local_node_config(config)?;
    let cell = either_cells
        .into_iter()
        .find(|c| c.cell().public_key().encode_base58_string() == cell_opts.public_key)
        .expect("Couldn't find cell with given public key");

    let chain_dir = cell
        .cell()
        .chain_directory()
        .expect("Cell doesn't have a path configured");
    std::fs::create_dir_all(&chain_dir)?;

    let chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;

    let mut block_count = 0;
    for block in chain_store.blocks_iter(0)? {
        block_count += 1;
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

    info!("Chain is valid. Analyzed {} blocks.", block_count);

    Ok(())
}

pub fn export_chain(
    _exo_opts: &options::ExoOptions,
    cell_opts: &options::CellOptions,
    export_opts: &options::ChainExportOptions,
) -> anyhow::Result<()> {
    let config = exocore_core::cell::node_config_from_yaml_file(&cell_opts.config)?;
    let (either_cells, _local_node) = Cell::new_from_local_node_config(config)?;
    let cell = either_cells
        .into_iter()
        .find(|c| c.cell().public_key().encode_base58_string() == cell_opts.public_key)
        .expect("Couldn't find cell with given public key");

    let chain_dir = cell
        .cell()
        .chain_directory()
        .expect("Cell doesn't have a path configured");
    std::fs::create_dir_all(&chain_dir)?;

    let chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;

    let file = std::fs::File::create(&export_opts.file)?;
    let mut file_buf = std::io::BufWriter::new(file);

    for block in chain_store.blocks_iter(0)? {
        let operations = block.operations_iter()?;
        for operation in operations {
            let operation_data = operation.exposed_data();
            let sized_frame = SizedFrameBuilder::new(operation_data.to_vec());
            sized_frame.write_to(&mut file_buf)?;
        }
    }

    file_buf.flush()?;

    Ok(())
}

pub fn import_chain(
    _exo_opts: &options::ExoOptions,
    cell_opts: &options::CellOptions,
    import_opts: &options::ChainImportOptions,
) -> anyhow::Result<()> {
    Ok(())
}
