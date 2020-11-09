use crate::{utils::edit_file, Options};
use clap::Clap;
use exocore_core::{
    cell::{LocalNodeConfigExt, NodeConfigExt},
    protos::core::{cell_application_config, node_cell_config, LocalNodeConfig, NodeConfig},
};

#[derive(Clap)]
pub struct ConfigOptions {
    #[clap(subcommand)]
    pub command: ConfigCommand,
}

#[derive(Clap)]
pub enum ConfigCommand {
    /// Edit node configuration.
    Edit,

    /// Print node configuration.
    Print(PrintOptions),

    /// Validate node configuration.
    Validate,
}

#[derive(Clap)]
pub struct PrintOptions {
    /// Print format.
    #[clap(long, default_value = "yaml")]
    pub format: String,

    /// Print configuration in `NodeConfig` format to be used to configure cell nodes.
    #[clap(long)]
    pub cell: bool,

    /// Inline configuration instead of pointing to external objects (cells / apps).
    #[clap(long)]
    pub inline: bool,

    /// Exclude applications schemas from configuration.
    #[clap(long)]
    pub exclude_app_schemas: bool,
}

pub fn handle_cmd(exo_opts: &Options, config_opts: &ConfigOptions) -> anyhow::Result<()> {
    match &config_opts.command {
        ConfigCommand::Edit => cmd_edit(&exo_opts, config_opts),
        ConfigCommand::Print(print_opts) => cmd_print(&exo_opts, config_opts, print_opts),
        ConfigCommand::Validate => cmd_validate(&exo_opts, config_opts),
    }
}

fn cmd_edit(exo_opts: &Options, _conf_opts: &ConfigOptions) -> anyhow::Result<()> {
    let config_path = exo_opts.conf_path();

    edit_file(config_path, |temp_path| {
        LocalNodeConfig::from_yaml_file(temp_path)?;
        Ok(())
    });

    Ok(())
}

fn cmd_validate(exo_opts: &Options, _conf_opts: &ConfigOptions) -> anyhow::Result<()> {
    // parse config
    let config = exo_opts.read_configuration();

    // create instance to validate the config
    let (_cells, _node) = exocore_core::cell::Cell::new_from_local_node_config(config)?;

    Ok(())
}

fn cmd_print(
    exo_opts: &Options,
    _conf_opts: &ConfigOptions,
    print_opts: &PrintOptions,
) -> anyhow::Result<()> {
    let node_config = exo_opts.read_configuration();

    if !print_opts.cell {
        cmd_print_node_config(node_config, print_opts);
    } else {
        cmd_print_cell_node_config(node_config);
    }

    Ok(())
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

    if print_opts.format == "json" {
        println!("{}", config.to_json().expect("Couldn't convert to json"));
    } else {
        println!("{}", config.to_yaml().expect("Couldn't convert to yaml"));
    }
}

fn cmd_print_cell_node_config(config: LocalNodeConfig) {
    let cell_node = NodeConfig {
        id: config.id,
        name: config.name,
        public_key: config.public_key,
        addresses: config.addresses,
    };

    println!("{}", cell_node.to_yaml().expect("Couldn't convert to yaml"));
}
