use log::LevelFilter;
use simple_logger::SimpleLogger;

#[macro_use]
extern crate log;

pub mod payload;
mod server;

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    SimpleLogger::new().with_level(LevelFilter::Info).init()?;

    let server = server::DiscoServer::new(8085);

    server.start().await?;

    Ok(())
}
