use std::net::{IpAddr, Ipv4Addr};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use terminald_server::{AuthConfig, ServerConfig};

use crate::client::ClientConfig;
use credentials::CredentialArgs;

mod credentials;

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

    #[arg(long, value_name = "HOST", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED), help = "Host address for the server to bind to")]
    host: IpAddr,

    #[command(flatten)]
    auth: CredentialArgs,

    #[arg(long, value_name = "PATH", conflicts_with_all = ["host", "port"], help = "Listen on a Unix socket instead of TCP")]
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

    #[arg(long, value_name = "HOST", default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED), help = "Host address for the server to bind to")]
    host: IpAddr,

    #[command(flatten)]
    auth: CredentialArgs,

    #[arg(long, value_name = "PATH", conflicts_with_all = ["host", "port"], help = "Listen on a Unix socket instead of TCP")]
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

    #[command(flatten)]
    auth: CredentialArgs,
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
                args.auth,
                args.unix_socket,
                args.command,
            )?)),
            Some(Subcommands::Client(args)) => Ok(CommandMode::Client(ClientConfig {
                connect: args.connect,
                credential: args.auth.resolve()?,
            })),
            Some(Subcommands::Version) => Ok(CommandMode::Version),
            None => Ok(CommandMode::Server(server_config(
                self.host,
                self.port,
                self.auth,
                self.unix_socket,
                self.server_command,
            )?)),
        }
    }
}

fn server_config(
    host: IpAddr,
    port: u16,
    auth: CredentialArgs,
    unix_socket: Option<std::path::PathBuf>,
    command: Vec<String>,
) -> Result<ServerConfig> {
    if command.is_empty() {
        bail!("command is required");
    }
    let mut config = ServerConfig::new(port, command);
    config.host = host;
    config.unix_socket = unix_socket;
    if let Some(credential) = auth.resolve()? {
        config.auth = AuthConfig::basic(credential);
    }
    Ok(config)
}

#[cfg(test)]
mod credential_tests;
#[cfg(test)]
mod tests;
