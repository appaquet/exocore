use exocore_core::cell::{LocalNodeConfigExt, NodeConfigExt};
use exocore_protos::core::{
    cell_application_config, node_cell_config, LocalNodeConfig, NodeConfig,
};

use crate::{
    cell::copy_local_node_to_cells,
    term::{confirm, print_info},
    utils::edit_string,
    Context,
};

#[derive(clap::Parser)]
pub struct ConfigOptions {
    #[clap(subcommand)]
    pub command: ConfigCommand,
}

#[derive(clap::Parser)]
pub enum ConfigCommand {
    /// Edit node configuration.
    Edit,

    /// Prints node configuration.
    Print(PrintOptions),

    /// Validates node configuration.
    Validate,
}

#[derive(clap::Parser)]
pub struct PrintOptions {
    /// Print configuration in `NodeConfig` format to be used to configure cell
    /// nodes.
    #[clap(long)]
    pub cell: bool,

    /// Inline configuration instead of pointing to external objects (cells /
    /// apps).
    #[clap(long)]
    pub inline: bool,

    /// Exclude applications schemas from configuration.
    #[clap(long)]
    pub exclude_app_schemas: bool,
}

pub fn handle_cmd(ctx: &Context, config_opts: &ConfigOptions) -> anyhow::Result<()> {
    match &config_opts.command {
        ConfigCommand::Edit => {
            cmd_edit(ctx, config_opts);
            Ok(())
        }
        ConfigCommand::Print(print_opts) => {
            cmd_print(ctx, config_opts, print_opts);
            Ok(())
        }
        ConfigCommand::Validate => cmd_validate(ctx, config_opts),
    }
}

fn cmd_edit(ctx: &Context, _conf_opts: &ConfigOptions) {
    let (local_node, _cells) = ctx.options.get_node_and_cells();

    let config_before = local_node.config().clone();
    let config_before_yaml = config_before
        .to_yaml_string()
        .expect("failed to serialize node config to yaml");

    let node_config_after = edit_string(config_before_yaml, |config_yaml| {
        let config_bytes = config_yaml.as_bytes();
        Ok(LocalNodeConfig::read_yaml(config_bytes)?)
    });

    if config_before.addresses == node_config_after.addresses
        && config_before.name == node_config_after.name
    {
        print_info("Node name or addresses didn't change. Not copying to cell.");
        return;
    }

    if confirm(ctx, "Copy configuration to cells?") {
        copy_local_node_to_cells(ctx, node_config_after);
    }
}

fn cmd_validate(ctx: &Context, _conf_opts: &ConfigOptions) -> anyhow::Result<()> {
    // create instance to validate the config
    let (_cells, _node) = ctx.options.get_node_and_cells();

    Ok(())
}

fn cmd_print(ctx: &Context, _conf_opts: &ConfigOptions, print_opts: &PrintOptions) {
    let (node, _cells) = ctx.options.get_node_and_cells();
    let node_config = node.config().clone();

    if !print_opts.cell {
        cmd_print_node_config(node_config, print_opts);
    } else {
        cmd_print_cell_node_config(node_config);
    }
}

fn cmd_print_node_config(config: LocalNodeConfig, print_opts: &PrintOptions) {
    let mut config = if print_opts.inline {
        config.inlined().expect("Couldn't inline configuration")
    } else {
        config
    };

    if print_opts.exclude_app_schemas {
        for cell in &mut config.cells {
            if let Some(node_cell_config::Location::Inline(cell_config)) = &mut cell.location {
                for app in &mut cell_config.apps {
                    if let Some(cell_application_config::Location::Inline(app_manifest)) =
                        &mut app.location
                    {
                        app_manifest.schemas.clear();
                    }
                }
            }
        }
    }

    println!(
        "{}",
        config.to_yaml_string().expect("Couldn't convert to yaml")
    );
}

fn cmd_print_cell_node_config(config: LocalNodeConfig) {
    let cell_node = NodeConfig {
        id: config.id,
        name: config.name,
        public_key: config.public_key,
        addresses: config.addresses,
    };

    println!(
        "{}",
        cell_node
            .to_yaml_string()
            .expect("Couldn't convert to yaml")
    );
}
