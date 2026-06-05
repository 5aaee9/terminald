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
        short = 'c',
        long = "credential",
        value_name = "USER:PASSWORD",
        help = "Basic authentication credential"
    )]
    credential: Option<String>,

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
        short = 'c',
        long = "credential",
        value_name = "USER:PASSWORD",
        help = "Basic authentication credential"
    )]
    credential: Option<String>,

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
                args.port,
                args.credential,
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
    use clap::{CommandFactory, Parser};

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

    #[test]
    fn parses_version() {
        let cli = Cli::try_parse_from(["terminald", "version"]).unwrap();
        assert!(matches!(cli.into_mode().unwrap(), CommandMode::Version));
    }

    #[test]
    fn top_level_help_describes_commands_and_arguments() {
        let help = Cli::command().render_long_help().to_string();

        assert!(help.contains("server"));
        assert!(help.contains("Run a terminald server"));
        assert!(help.contains("client"));
        assert!(help.contains("Connect to a terminald server"));
        assert!(help.contains("version"));
        assert!(help.contains("Print the terminald version"));
        assert!(help.contains("-p, --port <PORT>"));
        assert!(help.contains("Port for the server to listen on"));
        assert!(help.contains("-c, --credential <USER:PASSWORD>"));
        assert!(help.contains("Basic authentication credential"));
        assert!(help.contains("[SERVER_COMMAND]..."));
        assert!(help.contains("Command to run in the shared terminal"));
    }

    #[test]
    fn subcommand_help_describes_arguments() {
        let mut command = Cli::command();
        let server_help = command
            .find_subcommand_mut("server")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(server_help.contains("Run a terminald server"));
        assert!(server_help.contains("[COMMAND]..."));
        assert!(server_help.contains("Command to run in the shared terminal"));

        let client_help = command
            .find_subcommand_mut("client")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(client_help.contains("Connect to a terminald server"));
        assert!(client_help.contains("--connect <URL>"));
        assert!(client_help.contains("Server URL to connect to"));

        let version_help = command
            .find_subcommand_mut("version")
            .unwrap()
            .render_long_help()
            .to_string();
        assert!(version_help.contains("Print the terminald version"));
    }
}
