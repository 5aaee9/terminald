use std::net::{IpAddr, Ipv4Addr, SocketAddr};

use anyhow::{Context, Result};

use crate::app;
use crate::{AssetConfig, AuthConfig};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub command: Vec<String>,
    pub auth: AuthConfig,
    pub assets: AssetConfig,
}

pub async fn serve(config: ServerConfig) -> Result<()> {
    let address = SocketAddr::new(config.host, config.port);
    let listener = tokio::net::TcpListener::bind(address)
        .await
        .with_context(|| format!("bind terminald server on {address}"))?;
    axum::serve(listener, app(config))
        .await
        .context("serve terminald HTTP server")
}

impl ServerConfig {
    pub fn new(port: u16, command: Vec<String>) -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port,
            command,
            auth: AuthConfig::disabled(),
            assets: AssetConfig::embedded(),
        }
    }
}
