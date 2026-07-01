# Server Bind Host Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a server bind-host CLI option while preserving the default bind address of `0.0.0.0`.

**Architecture:** Keep bind-host configuration at the CLI boundary by parsing `--host <HOST>` as `std::net::IpAddr` and passing it into the existing `ServerConfig.host` field. The server crate already binds `SocketAddr::new(config.host, config.port)`, so no server runtime API or listener behavior change is needed.

**Tech Stack:** Rust 2024, clap derive, anyhow, existing `terminald-cli` and `terminald-server` crates.

## Global Constraints

- `terminald server ...` and implicit `terminald ...` run server mode.
- A trailing server command is required; Terminald must not fall back to `$SHELL` when no command is provided.
- Server mode binds to host `0.0.0.0` by default while preserving the requested port.
- `--host <HOST>` must be a long-only server-mode CLI argument parsed as `std::net::IpAddr`.
- Do not add hostname resolution or accept DNS names.
- Do not add environment-variable configuration.
- Do not change server bind error handling.
- Do not change client connection URL behavior.
- Do not change authentication, routes, frontend assets, or WebSocket protocol behavior.
- Source files under `crates/` and `frontend/src/` must stay below 400 LOC.
- Preserve lower-level error context across runtime, listener, configuration, network, and system-call boundaries.
- Do not intentionally modify or commit generated frontend assets under `crates/terminald-server/assets/dist`.
- Under this `$sdd-workflow`, implementation workers must not create git commits. The parent creates one final reviewed commit after implementation review, documentation updates, and fresh verification.

---

## File Structure

- Modify `crates/terminald-cli/src/args.rs`: add the `--host <HOST>` parser field to implicit and explicit server mode, pass it into `server_config`, and add parser/help tests.
- Modify `README.md` only after the implementation review gate approves the code change: document the default bind host and the `--host` example.
- Do not modify `crates/terminald-server/src/server.rs`; `ServerConfig::new` already defaults `host` to `0.0.0.0`, and the listener already binds `config.host`.

## Task 1: Add Server Bind Host CLI Argument

**Files:**
- Modify: `crates/terminald-cli/src/args.rs`

**Interfaces:**
- Consumes: existing `ServerConfig::new(port: u16, command: Vec<String>) -> ServerConfig` with public `host: IpAddr` field.
- Produces: CLI parser behavior where explicit and implicit server modes set `ServerConfig.host` from `--host <HOST>` or default to `IpAddr::V4(Ipv4Addr::UNSPECIFIED)`.

- [ ] **Step 1: Write failing parser tests**

In `crates/terminald-cli/src/args.rs`, extend the test module imports for IPv6 and clap error-kind assertions:

```rust
use std::net::Ipv6Addr;
use clap::error::ErrorKind;
```

Add these tests inside `mod tests`:

```rust
#[test]
fn parses_explicit_server_host() {
    let cli = Cli::try_parse_from([
        "terminald",
        "server",
        "--host",
        "127.0.0.1",
        "bash",
    ])
    .unwrap();
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
    let explicit_error = Cli::try_parse_from([
        "terminald",
        "server",
        "--host",
        "localhost",
        "bash",
    ])
    .unwrap_err();
    assert_eq!(explicit_error.kind(), ErrorKind::ValueValidation);

    let implicit_error = Cli::try_parse_from(["terminald", "--host", "localhost", "bash"])
        .unwrap_err();
    assert_eq!(implicit_error.kind(), ErrorKind::ValueValidation);
}
```

Update `top_level_help_describes_commands_and_arguments` to assert both help strings:

```rust
assert!(help.contains("--host <HOST>"));
assert!(help.contains("Host address for the server to bind to"));
```

Update `subcommand_help_describes_arguments` after rendering `server_help`:

```rust
assert!(server_help.contains("--host <HOST>"));
assert!(server_help.contains("Host address for the server to bind to"));
```

- [ ] **Step 2: Run focused parser tests and verify RED**

Run:

```bash
cargo test -p terminald-cli args::tests
```

Expected before implementation: the new `--host` tests fail because clap does not recognize `--host`; the existing help tests also fail because help output does not contain `--host <HOST>`.

- [ ] **Step 3: Add `--host <HOST>` to top-level implicit server args**

In the `Cli` struct in `crates/terminald-cli/src/args.rs`, add this field after `port`:

```rust
#[arg(
    long = "host",
    value_name = "HOST",
    default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    help = "Host address for the server to bind to"
)]
host: IpAddr,
```

Update the implicit server-mode call to `server_config` so it passes `self.host`:

```rust
Ok(CommandMode::Server(server_config(
    self.host,
    self.port,
    self.credential,
    self.server_command,
)?))
```

- [ ] **Step 4: Add `--host <HOST>` to explicit server subcommand args**

In the `ServerArgs` struct in `crates/terminald-cli/src/args.rs`, add this field after `port`:

```rust
#[arg(
    long = "host",
    value_name = "HOST",
    default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED),
    help = "Host address for the server to bind to"
)]
host: IpAddr,
```

Update the explicit `server` subcommand call to `server_config` so it passes `args.host`:

```rust
Some(Subcommands::Server(args)) => Ok(CommandMode::Server(server_config(
    args.host,
    args.port,
    args.credential,
    args.command,
)?)),
```

- [ ] **Step 5: Thread the parsed host into `ServerConfig`**

Change the `server_config` signature from:

```rust
fn server_config(
    port: u16,
    credential: Option<String>,
    command: Vec<String>,
) -> Result<ServerConfig> {
```

to:

```rust
fn server_config(
    host: IpAddr,
    port: u16,
    credential: Option<String>,
    command: Vec<String>,
) -> Result<ServerConfig> {
```

Keep the command-required check, then assign the parsed host:

```rust
let mut config = ServerConfig::new(port, command);
config.host = host;
```

- [ ] **Step 6: Run focused parser tests and verify GREEN**

Run:

```bash
cargo test -p terminald-cli args::tests
```

Expected after implementation: all `args::tests` pass, including default-host, custom IPv4 host, custom IPv6 host, invalid host rejection, and help-output coverage.

- [ ] **Step 7: Run file-length check**

Run:

```bash
wc -l crates/terminald-cli/src/args.rs
```

Expected: the file remains below 400 LOC.

- [ ] **Step 8: Implementation worker report**

Write a report containing:

- RED command and failing output summary.
- GREEN command and passing output summary.
- Changed files.
- Confirmation that no git commit was created by the worker.
- Any concerns or residual risks.

## Post-Implementation Documentation Step

This step must happen only after the independent implementation reviewer returns `VERDICT: APPROVE` for Task 1.

Modify `README.md` in the Server section to state that Terminald binds `0.0.0.0` by default and show a localhost bind example:

````markdown
By default the server binds to `0.0.0.0`. Use `--host` with an IP literal to bind a specific interface, for example localhost only:

```bash
terminald server --host 127.0.0.1 -p 7681 bash
```
````

After the README update, run fresh final verification:

```bash
cargo fmt --check
cargo test -p terminald-cli
cargo test --workspace
```

Review `git status --short` and `git diff --check` before creating the final commit.
