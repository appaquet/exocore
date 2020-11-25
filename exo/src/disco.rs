use crate::Options;
use clap::Clap;

#[derive(Clap)]
pub enum DiscoveryCommand {
    // Start a discovery service daemon.
    Daemon(DaemonOptions),
}

#[derive(Clap)]
pub struct DaemonOptions {
    #[clap(long, default_value = "8085")]
    port: u16,
}

pub async fn cmd_daemon(_opts: &Options, cmd: &DiscoveryCommand) -> anyhow::Result<()> {
    match cmd {
        DiscoveryCommand::Daemon(daemon_opts) => start_daemon(daemon_opts).await,
    }
}

async fn start_daemon(opts: &DaemonOptions) -> anyhow::Result<()> {
    let server = exocore_discovery::Server::new(opts.port);
    server.start().await?;

    Ok(())
}
