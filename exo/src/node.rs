use std::path::Path;

use exocore_core::cell::{LocalNode, LocalNodeConfigExt};
use exocore_protos::core::{LocalNodeConfig, NodeAddresses};

use crate::{term::*, Context};

#[derive(clap::Parser)]
pub struct NodeOptions {
    #[clap(subcommand)]
    pub command: NodeCommand,
}

#[derive(clap::Parser)]
pub enum NodeCommand {
    /// Initializes the node.
    Init(InitOptions),
}

#[derive(clap::Parser)]
pub struct InitOptions {
    /// Name of the node.
    #[clap(long)]
    pub name: Option<String>,
}

pub fn handle_cmd(ctx: &Context, node_opts: &NodeOptions) -> anyhow::Result<()> {
    match &node_opts.command {
        NodeCommand::Init(init_opts) => cmd_init(ctx, init_opts),
    }
}

fn cmd_init(ctx: &Context, init_opts: &InitOptions) -> anyhow::Result<()> {
    let node_dir = ctx.options.node_directory();
    if node_dir.exists(Path::new(exocore_core::cell::NODE_CONFIG_FILE)) {
        panic!(
            "Cannot initialize node. A node configuration already exists in '{:?}'",
            node_dir.as_os_path().unwrap()
        );
    }

    print_step("Initializing node directory");
    let home_path = ctx.options.dir_path();
    if !home_path.exists() {
        print_action(format!("Creating directory {}", style_value(&home_path)));
        std::fs::create_dir_all(home_path).expect("Couldn't create home directory");
    }

    let local_node = LocalNode::generate();
    let node = local_node.node();

    let mut node_name = node.name().to_string();
    if init_opts.name.is_none() {
        print_spacer();
        node_name = dialoguer::Input::with_theme(ctx.dialog_theme.as_ref())
            .with_prompt("Enter the name of the node")
            .default(node.name().to_string())
            .interact_text()?;
    }

    // generate port randomly
    let port_rand = 10 + rand::random::<u8>() % 90;

    let local_node_config = LocalNodeConfig {
        name: node_name,
        addresses: Some(NodeAddresses {
            p2p: vec![
                format!("/ip4/127.0.0.1/tcp/33{}", port_rand),
                format!("/ip4/127.0.0.1/tcp/34{}/ws", port_rand),
            ],
            http: vec![format!("http://127.0.0.1:80{}", port_rand)],
        }),
        listen_addresses: Some(NodeAddresses {
            p2p: vec![
                format!("/ip4/0.0.0.0/tcp/33{}", port_rand),
                format!("/ip4/0.0.0.0/tcp/34{}/ws", port_rand),
            ],
            http: vec![format!("http://0.0.0.0:80{}", port_rand)],
        }),

        ..local_node.config().clone()
    };

    print_action("Writing node configuration");
    let config_file = node_dir
        .open_create(Path::new(exocore_core::cell::NODE_CONFIG_FILE))
        .expect("Couldn't create node configuration file");
    local_node_config.to_yaml_writer(config_file)?;

    print_success(format!(
        "Node {} with public key {} created",
        style_value(&local_node_config.name),
        style_value(&local_node_config.public_key),
    ));
    Ok(())
}
