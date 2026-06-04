use std::net::{IpAddr, Ipv4Addr};

use anyhow::{Result, bail};
use clap::{Args, Parser, Subcommand};
use terminald_server::{AuthConfig, Credential, ServerConfig};

use crate::client::ClientConfig;

#[derive(Debug, Parser)]
#[command(name = "terminald")]
pub struct Cli {
    #[command(subcommand)]
    command: Option<Subcommands>,

    #[arg(short = 'p', long = "port", default_value_t = 7681)]
    port: u16,

    #[arg(short = 'c', long = "credential")]
    credential: Option<String>,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    server_command: Vec<String>,
}

#[derive(Debug, Subcommand)]
enum Subcommands {
    Server(ServerArgs),
    Client(ClientArgs),
}

#[derive(Debug, Args)]
struct ServerArgs {
    #[arg(short = 'p', long = "port", default_value_t = 7681)]
    port: u16,

    #[arg(short = 'c', long = "credential")]
    credential: Option<String>,

    #[arg(trailing_var_arg = true, allow_hyphen_values = true)]
    command: Vec<String>,
}

#[derive(Debug, Args)]
struct ClientArgs {
    #[arg(long = "connect")]
    connect: String,

    #[arg(short = 'c', long = "credential")]
    credential: Option<String>,
}

#[derive(Debug)]
pub enum CommandMode {
    Server(ServerConfig),
    Client(ClientConfig),
}

impl Cli {
    pub fn into_mode(self) -> Result<CommandMode> {
        match self.command {
            Some(Subcommands::Server(args)) => Ok(CommandMode::Server(server_config(
                args.port,
                args.credential,
                args.command,
            )?)),
            Some(Subcommands::Client(args)) => Ok(CommandMode::Client(ClientConfig {
                connect: args.connect,
                credential: parse_credential(args.credential)?,
            })),
            None => {
                if self.server_command.is_empty() {
                    bail!("command is required");
                }
                Ok(CommandMode::Server(server_config(
                    self.port,
                    self.credential,
                    self.server_command,
                )?))
            }
        }
    }
}

fn server_config(
    port: u16,
    credential: Option<String>,
    command: Vec<String>,
) -> Result<ServerConfig> {
    if command.is_empty() {
        bail!("command is required");
    }
    let mut config = ServerConfig::new(port, command);
    config.host = IpAddr::V4(Ipv4Addr::UNSPECIFIED);
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
mod tests {
    use super::*;
    use clap::Parser;

    #[test]
    fn parses_explicit_server() {
        let cli = Cli::try_parse_from([
            "terminald",
            "server",
            "-p",
            "7681",
            "-c",
            "user:pass",
            "bash",
        ])
        .unwrap();
        let CommandMode::Server(config) = cli.into_mode().unwrap() else {
            panic!("expected server mode");
        };
        assert_eq!(config.port, 7681);
        assert_eq!(config.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(config.command, vec!["bash"]);
    }

    #[test]
    fn parses_implicit_server() {
        let cli =
            Cli::try_parse_from(["terminald", "-p", "9000", "-c", "user:pass", "bash"]).unwrap();
        let CommandMode::Server(config) = cli.into_mode().unwrap() else {
            panic!("expected server mode");
        };
        assert_eq!(config.port, 9000);
        assert_eq!(config.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
        assert_eq!(config.command, vec!["bash"]);
    }

    #[test]
    fn server_command_is_required() {
        let cli = Cli::try_parse_from(["terminald", "server"]).unwrap();
        assert!(
            cli.into_mode()
                .unwrap_err()
                .to_string()
                .contains("command is required")
        );
        let cli = Cli::try_parse_from(["terminald"]).unwrap();
        assert!(
            cli.into_mode()
                .unwrap_err()
                .to_string()
                .contains("command is required")
        );
    }

    #[test]
    fn parses_client() {
        let cli = Cli::try_parse_from([
            "terminald",
            "client",
            "--connect",
            "http://127.0.0.1:7681",
            "-c",
            "user:pass",
        ])
        .unwrap();
        let CommandMode::Client(config) = cli.into_mode().unwrap() else {
            panic!("expected client mode");
        };
        assert_eq!(config.connect, "http://127.0.0.1:7681");
        assert!(config.credential.is_some());
    }
}
