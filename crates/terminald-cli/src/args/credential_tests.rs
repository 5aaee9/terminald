use super::*;
use std::fs;
use tempfile::TempDir;

fn cli_with_file(mode: &str, path: &std::path::Path) -> Cli {
    let mut args = vec!["terminald"];
    if !mode.is_empty() {
        args.push(mode);
    }
    args.extend(["--credential-file", path.to_str().unwrap()]);
    if mode == "client" {
        args.extend(["--connect", "http://localhost:7681"]);
    } else {
        args.push("sh");
    }
    Cli::try_parse_from(args).unwrap()
}

#[test]
fn credential_files_work_in_all_modes() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("auth");
    for ending in ["", "\n", "\r\n"] {
        // Spaces and password colons are significant; only the line ending is removed.
        fs::write(&path, format!("alice: secret:phrase {ending}")).unwrap();
        for mode in ["", "server", "client"] {
            match cli_with_file(mode, &path).into_mode().unwrap() {
                CommandMode::Server(config) => {
                    let mut headers = http::HeaderMap::new();
                    use base64::Engine;
                    let encoded =
                        base64::engine::general_purpose::STANDARD.encode("alice: secret:phrase ");
                    headers.insert(
                        http::header::AUTHORIZATION,
                        format!("Basic {encoded}").parse().unwrap(),
                    );
                    assert!(config.auth.validate_headers(&headers));
                }
                CommandMode::Client(config) => assert_eq!(
                    config.credential.unwrap().to_basic_pair(),
                    "alice: secret:phrase "
                ),
                CommandMode::Version => panic!("unexpected version mode"),
            }
        }
    }
}

#[test]
fn invalid_credential_files_fail_closed_without_revealing_contents() {
    let dir = TempDir::new().unwrap();
    let path = dir.path().join("auth");
    for content in [
        b"".as_slice(),
        b"secret-without-colon",
        b"alice:secret\nother:value",
        b"alice:secret\n\n",
        b"alice:secret\r",
        b"alice:secret\0",
        b"alice:\xff",
    ] {
        fs::write(&path, content).unwrap();
        for mode in ["", "server", "client"] {
            let error = cli_with_file(mode, &path).into_mode().unwrap_err();
            let message = format!("{error:#}");
            assert!(
                !message.contains("secret"),
                "credential contents leaked in error"
            );
        }
    }
    fs::remove_file(&path).unwrap();
    for mode in ["", "server", "client"] {
        let error = cli_with_file(mode, &path).into_mode().unwrap_err();
        assert!(error.chain().count() > 1, "file IO error lost its cause");
    }
}

#[test]
fn credential_sources_conflict_in_all_modes() {
    for mode in ["", "server", "client"] {
        let mut args = vec!["terminald"];
        if !mode.is_empty() {
            args.push(mode);
        }
        args.extend(["-c", "alice:secret", "--credential-file", "/unused"]);
        if mode == "client" {
            args.extend(["--connect", "http://localhost"]);
        } else {
            args.push("sh");
        }
        assert_eq!(
            Cli::try_parse_from(args).unwrap_err().kind(),
            clap::error::ErrorKind::ArgumentConflict
        );
    }
}

#[test]
fn unix_socket_conflicts_with_explicit_tcp_options() {
    for mode in ["", "server"] {
        for tcp in [["--host", "127.0.0.1"], ["--port", "7681"]] {
            let mut args = vec!["terminald"];
            if !mode.is_empty() {
                args.push(mode);
            }
            args.extend(["--unix-socket", "/unused.sock"]);
            args.extend(tcp);
            args.push("sh");
            assert_eq!(
                Cli::try_parse_from(args).unwrap_err().kind(),
                clap::error::ErrorKind::ArgumentConflict
            );
        }
    }
}

#[test]
fn parsed_cli_debug_redacts_credentials() {
    for args in [
        vec!["terminald", "-c", "alice:secret", "sh"],
        vec!["terminald", "server", "-c", "alice:secret", "sh"],
        vec![
            "terminald",
            "client",
            "-c",
            "alice:secret",
            "--connect",
            "http://localhost",
        ],
    ] {
        let cli = Cli::try_parse_from(args).unwrap();
        let debug = format!("{cli:?}");
        assert!(!debug.contains("alice"));
        assert!(!debug.contains("secret"));
    }
}
