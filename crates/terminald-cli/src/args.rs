use std::net::{IpAddr, Ipv4Addr};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use terminald_server::{AuthConfig, Credential, ServerConfig};

use crate::client::ClientConfig;

#[derive(Debug, Parser)]
#[command(name = "terminald", about = "Share a terminal over the web")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Subcommands>,

    #[arg(
        short = 'p',
        long = "port",
        default_value_t = 7681,
        help = "Port for the server to listen on"
    )]
    port: u16,

    #[arg(
        long = "host",
        value_name = "HOST",
        default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        help = "Host address for the server to bind to"
    )]
    host: IpAddr,

    #[arg(
        short = 'c',
        long = "credential",
        value_name = "USER:PASSWORD",
        help = "Basic authentication credential"
    )]
    credential: Option<String>,

    #[arg(long, conflicts_with = "credential")]
    credential_file: Option<std::path::PathBuf>,

    #[arg(long, conflicts_with_all = ["host", "port"])]
    unix_socket: Option<std::path::PathBuf>,

    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Command to run in the shared terminal"
    )]
    server_command: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum Subcommands {
    #[command(about = "Run a terminald server")]
    Server(ServerArgs),
    #[command(about = "Connect to a terminald server")]
    Client(ClientArgs),
    #[command(about = "Print the terminald version")]
    Version,
}

#[derive(Debug, Args)]
struct ServerArgs {
    #[arg(
        short = 'p',
        long = "port",
        default_value_t = 7681,
        help = "Port for the server to listen on"
    )]
    port: u16,

    #[arg(
        long = "host",
        value_name = "HOST",
        default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        help = "Host address for the server to bind to"
    )]
    host: IpAddr,

    #[arg(
        short = 'c',
        long = "credential",
        value_name = "USER:PASSWORD",
        help = "Basic authentication credential"
    )]
    credential: Option<String>,

    #[arg(long, conflicts_with = "credential")]
    credential_file: Option<std::path::PathBuf>,

    #[arg(long, conflicts_with_all = ["host", "port"])]
    unix_socket: Option<std::path::PathBuf>,

    #[arg(
        trailing_var_arg = true,
        allow_hyphen_values = true,
        help = "Command to run in the shared terminal"
    )]
    command: Vec<String>,
}

#[derive(Debug, Args)]
struct ClientArgs {
    #[arg(
        long = "connect",
        value_name = "URL",
        help = "Server URL to connect to"
    )]
    connect: String,

    #[arg(
        short = 'c',
        long = "credential",
        value_name = "USER:PASSWORD",
        help = "Basic authentication credential"
    )]
    credential: Option<String>,
}

#[derive(Debug)]
pub enum CommandMode {
    Server(ServerConfig),
    Client(ClientConfig),
    Version,
}

impl Cli {
    pub fn into_mode(self) -> Result<CommandMode> {
        match self.command {
            Some(Subcommands::Server(args)) => Ok(CommandMode::Server(server_config(
                args.host,
                args.port,
                args.credential,
                args.credential_file,
                args.unix_socket,
                args.command,
            )?)),
            Some(Subcommands::Client(args)) => Ok(CommandMode::Client(ClientConfig {
                connect: args.connect,
                credential: parse_credential(args.credential)?,
            })),
            Some(Subcommands::Version) => Ok(CommandMode::Version),
            None => {
                if self.server_command.is_empty() {
                    bail!("command is required");
                }
                Ok(CommandMode::Server(server_config(
                    self.host,
                    self.port,
                    self.credential,
                    self.credential_file,
                    self.unix_socket,
                    self.server_command,
                )?))
            }
        }
    }
}

fn server_config(
    host: IpAddr,
    port: u16,
    credential: Option<String>,
    credential_file: Option<std::path::PathBuf>,
    unix_socket: Option<std::path::PathBuf>,
    command: Vec<String>,
) -> Result<ServerConfig> {
    if command.is_empty() {
        bail!("command is required");
    }
    let mut config = ServerConfig::new(port, command);
    config.host = host;
    config.unix_socket = unix_socket;
    let credential = match credential_file {
        Some(path) => {
            use anyhow::Context;
            let value = std::fs::read_to_string(&path)
                .with_context(|| format!("read credential file {}", path.display()))?;
            Some(value.trim_end_matches(['\r', '\n']).to_owned())
        }
        None => credential,
    };
    if let Some(credential) = parse_credential(credential)? {
        config.auth = AuthConfig::basic(credential);
    }
    Ok(config)
}

pub fn parse_credential(value: Option<String>) -> Result<Option<Credential>> {
    value
        .map(|value| {
            Credential::new(&value)
                .ok_or_else(|| anyhow::anyhow!("credential must be user:password"))
        })
        .transpose()
}

#[cfg(test)]
mod tests;
