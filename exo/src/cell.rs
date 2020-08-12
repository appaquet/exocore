use crate::options;
use exocore_chain::block::{Block, BlockOperations, BlockOwned};
use exocore_chain::chain::ChainStore;
use exocore_chain::{DirectoryChainStore, DirectoryChainStoreConfig};
use exocore_core::cell::{Cell, EitherCell};
use exocore_core::{
    framing::{sized::SizedFrameReaderIterator, FrameReader},
    protos::{core::LocalNodeConfig, generated::data_chain_capnp::block_header},
};
use std::io::Write;

pub fn create_genesis_block(
    _exo_opts: &options::ExoOptions,
    cell_opts: &options::CellOptions,
) -> anyhow::Result<()> {
    let (_, cell) = get_cell(cell_opts);
    let full_cell = cell.unwrap_full();

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
    let (_, cell) = get_cell(cell_opts);

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
    let (_, cell) = get_cell(cell_opts);

    let chain_dir = cell
        .cell()
        .chain_directory()
        .expect("Cell doesn't have a path configured");
    std::fs::create_dir_all(&chain_dir)?;

    let chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;

    let file = std::fs::File::create(&export_opts.file).expect("Couldn't open exported file");
    let mut file_buf = std::io::BufWriter::new(file);

    for block in chain_store.blocks_iter(0)? {
        let operations = block.operations_iter()?;
        for operation_frame in operations {
            operation_frame.copy_to(&mut file_buf)?;
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
    let (_, cell) = get_cell(cell_opts);
    let full_cell = cell.unwrap_full();

    let chain_dir = full_cell
        .chain_directory()
        .expect("Cell doesn't have a path configured");
    std::fs::create_dir_all(&chain_dir)?;

    let mut chain_store =
        DirectoryChainStore::create_or_open(DirectoryChainStoreConfig::default(), &chain_dir)?;
    if chain_store.get_last_block()?.is_some() {
        panic!("Chain is already initialized");
    }

    let file = std::fs::File::open(&import_opts.file).expect("Couldn't open imported file");

    let genesis_block = exocore_chain::block::BlockOwned::new_genesis(&full_cell)?;
    chain_store.write_block(&genesis_block)?;

    let mut previous_block = genesis_block;

    let mut operations_buffer = Vec::new();

    let operation_frames_iter = SizedFrameReaderIterator::new(file);
    for operation_frame in operation_frames_iter {
        let operation =
            exocore_chain::operation::read_operation_frame(operation_frame.frame.whole_data())?;

        // let reader = operation.get_reader()?;
        // println!("Got one: {}", reader.get_operation_id());

        // TODO: Filter only entry
        operations_buffer.push(operation.to_owned());

        if operations_buffer.len() > 10 {
            // TODO: make sure it's sorted

            let block_op_id = 0;
            let operations = BlockOperations::from_operations(operations_buffer.iter())?;
            let block = BlockOwned::new_with_prev_block(
                &full_cell,
                &previous_block,
                block_op_id,
                operations,
            )?;
            chain_store.write_block(&block)?;

            previous_block = block;

            operations_buffer.clear();
        }
    }

    if operations_buffer.len() > 10 {
        // TODO:
    }

    Ok(())
}

fn get_cell(cell_opts: &options::CellOptions) -> (LocalNodeConfig, EitherCell) {
    let config = exocore_core::cell::node_config_from_yaml_file(&cell_opts.config)
        .expect("Error parsing configuration");
    let (either_cells, _local_node) =
        Cell::new_from_local_node_config(config.clone()).expect("Couldn't create cell from config");
    let cell = either_cells
        .into_iter()
        .find(|c| c.cell().public_key().encode_base58_string() == cell_opts.public_key)
        .expect("Couldn't find cell with given public key");

    (config, cell)
}
