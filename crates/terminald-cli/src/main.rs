mod args;
mod client;
mod server;
mod terminal;

use anyhow::Result;
use args::{Cli, CommandMode};
use clap::Parser;

const MIN_RUNTIME_WORKER_THREADS: usize = 8;

fn main() -> Result<()> {
    tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .worker_threads(runtime_worker_threads())
        .build()?
        .block_on(run())
}

async fn run() -> Result<()> {
    match Cli::parse().into_mode()? {
        CommandMode::Server(config) => server::run(config).await,
        CommandMode::Client(config) => client::run(config).await,
        CommandMode::Version => {
            println!("{}", env!("CARGO_PKG_VERSION"));
            Ok(())
        }
    }
}

fn runtime_worker_threads() -> usize {
    std::thread::available_parallelism()
        .map(usize::from)
        .unwrap_or(1)
        .max(MIN_RUNTIME_WORKER_THREADS)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn runtime_worker_threads_have_a_floor_for_many_pty_sessions() {
        assert!(runtime_worker_threads() >= 8);
    }
}
