use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::app;
use crate::{AssetConfig, AuthConfig};

#[derive(Clone, Debug)]
pub struct ServerConfig {
    pub unix_socket: Option<PathBuf>,
    pub host: IpAddr,
    pub port: u16,
    pub command: Vec<String>,
    pub auth: AuthConfig,
    pub assets: AssetConfig,
}

pub async fn serve(config: ServerConfig) -> Result<()> {
    if let Some(path) = &config.unix_socket {
        let listener = tokio::net::UnixListener::bind(path)
            .with_context(|| format!("bind terminald Unix socket {}", path.display()))?;
        return axum::serve(listener, app(config))
            .await
            .context("serve terminald Unix HTTP server");
    }
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
            unix_socket: None,
            host: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port,
            command,
            auth: AuthConfig::disabled(),
            assets: AssetConfig::embedded(),
        }
    }
}
