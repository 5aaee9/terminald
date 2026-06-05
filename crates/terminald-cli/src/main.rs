mod args;
mod client;
mod server;
mod terminal;

use anyhow::Result;
use args::{Cli, CommandMode};
use clap::Parser;

#[tokio::main]
async fn main() -> Result<()> {
    match Cli::parse().into_mode()? {
        CommandMode::Server(config) => server::run(config).await,
        CommandMode::Client(config) => client::run(config).await,
        CommandMode::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}
