use anyhow::Result;
use terminald_server::{ServerConfig, serve};

pub async fn run(config: ServerConfig) -> Result<()> {
    serve(config).await
}
