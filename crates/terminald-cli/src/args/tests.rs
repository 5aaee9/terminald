use super::*;
use clap::{CommandFactory, Parser, error::ErrorKind};
use std::net::Ipv6Addr;

#[test]
fn credential_file_and_unix_socket() {
    let path = std::env::temp_dir().join(format!("terminald-credential-{}", std::process::id()));
    std::fs::write(&path, "test:secret\n").unwrap();
    let cli = Cli::try_parse_from([
        "terminald",
        "server",
        "--credential-file",
        path.to_str().unwrap(),
        "--unix-socket",
        "/run/test.sock",
        "bash",
    ])
    .unwrap();
    let CommandMode::Server(config) = cli.into_mode().unwrap() else {
        panic!("server expected")
    };
    assert!(config.auth.is_enabled());
    assert_eq!(
        config.unix_socket.unwrap(),
        std::path::PathBuf::from("/run/test.sock")
    );
    std::fs::remove_file(&path).unwrap();
    assert!(
        Cli::try_parse_from([
            "terminald",
            "-c",
            "u:p",
            "--credential-file",
            "/missing",
            "bash"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "terminald",
            "--unix-socket",
            "/run/test.sock",
            "--host",
            "127.0.0.1",
            "bash"
        ])
        .is_err()
    );
    assert!(
        Cli::try_parse_from([
            "terminald",
            "--credential-file",
            "/missing/terminald-secret",
            "bash"
        ])
        .unwrap()
        .into_mode()
        .is_err()
    );
}

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
    let cli = Cli::try_parse_from(["terminald", "-p", "9000", "-c", "user:pass", "bash"]).unwrap();
    let CommandMode::Server(config) = cli.into_mode().unwrap() else {
        panic!("expected server mode");
    };
    assert_eq!(config.port, 9000);
    assert_eq!(config.host, IpAddr::V4(Ipv4Addr::UNSPECIFIED));
    assert_eq!(config.command, vec!["bash"]);
}

#[test]
fn parses_explicit_server_host() {
    let cli = Cli::try_parse_from(["terminald", "server", "--host", "127.0.0.1", "bash"]).unwrap();
    let CommandMode::Server(config) = cli.into_mode().unwrap() else {
        panic!("expected server mode");
    };
    assert_eq!(config.host, IpAddr::V4(Ipv4Addr::LOCALHOST));
    assert_eq!(config.port, 7681);
    assert_eq!(config.command, vec!["bash"]);
}

#[test]
fn parses_implicit_server_ipv6_host() {
    let cli = Cli::try_parse_from(["terminald", "--host", "::1", "bash"]).unwrap();
    let CommandMode::Server(config) = cli.into_mode().unwrap() else {
        panic!("expected server mode");
    };
    assert_eq!(config.host, IpAddr::V6(Ipv6Addr::LOCALHOST));
    assert_eq!(config.port, 7681);
    assert_eq!(config.command, vec!["bash"]);
}

#[test]
fn invalid_server_host_is_rejected() {
    let explicit_error =
        Cli::try_parse_from(["terminald", "server", "--host", "localhost", "bash"]).unwrap_err();
    assert_eq!(explicit_error.kind(), ErrorKind::ValueValidation);

    let implicit_error =
        Cli::try_parse_from(["terminald", "--host", "localhost", "bash"]).unwrap_err();
    assert_eq!(implicit_error.kind(), ErrorKind::ValueValidation);
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
    assert!(help.contains("--host <HOST>"));
    assert!(help.contains("Host address for the server to bind to"));
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
    assert!(server_help.contains("--host <HOST>"));
    assert!(server_help.contains("Host address for the server to bind to"));

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
