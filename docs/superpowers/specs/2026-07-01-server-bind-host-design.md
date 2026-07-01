# Server Bind Host Design

## Scope

Add a configurable server bind host to Terminald's CLI while keeping the default server bind address as `0.0.0.0`.

This change covers only server-mode argument parsing and the value passed into the existing `terminald_server::ServerConfig`. It does not change routing, authentication, WebSocket paths, frontend behavior, PTY sessions, or client-mode connection handling.

## Current Behavior

- `ServerConfig::new(port, command)` defaults `host` to `0.0.0.0`.
- CLI server parsing also forces `config.host` to `0.0.0.0` after constructing `ServerConfig`.
- `terminald server ...` and implicit server mode (`terminald ...`) expose `--port` and `--credential`, but not a host option.
- The server runner binds `SocketAddr::new(config.host, config.port)` and already preserves bind errors with context.

## Approaches Considered

1. Add a long-only `--host <HOST>` argument to both server-mode CLI surfaces and parse it as `std::net::IpAddr`.
   - Pros: matches the existing `ServerConfig.host` type, gives clear validation at the CLI boundary, supports IPv4 and IPv6 literals, and avoids hostname resolution inside server startup.
   - Cons: users cannot pass DNS names such as `localhost`.

2. Change `ServerConfig.host` from `IpAddr` to `String` or a hostname-aware type and resolve hostnames during bind.
   - Pros: accepts names like `localhost`.
   - Cons: expands the server API, adds DNS-resolution behavior that is not currently needed, and makes bind errors less direct.

3. Add an environment variable only.
   - Pros: no CLI shape change.
   - Cons: does not meet the requirement that the host can be modified with CLI arguments.

Chosen approach: option 1. The existing server configuration already models the bind host as an IP address, so the CLI should expose that same boundary directly.

## CLI Design

Add `--host <HOST>` to:

- top-level implicit server mode: `terminald --host 127.0.0.1 -p 7681 bash`
- explicit server mode: `terminald server --host 127.0.0.1 -p 7681 bash`

Behavior:

- Default value is `0.0.0.0` for both server modes.
- `HOST` is parsed as `std::net::IpAddr` by clap.
- Valid examples include `0.0.0.0`, `127.0.0.1`, `::`, and `::1`.
- Invalid values fail during CLI parsing with clap's typed value error.
- No short flag is added. A long-only option avoids consuming another short option and keeps the CLI explicit.

Client mode remains unchanged. `terminald client --connect ...` does not accept or use `--host`.

## Implementation Design

Modify `crates/terminald-cli/src/args.rs`:

- Keep the existing `use std::net::{IpAddr, Ipv4Addr};` imports.
- Add a `host: IpAddr` field to `Cli` and `ServerArgs` with `long = "host"`, `value_name = "HOST"`, `default_value_t = IpAddr::V4(Ipv4Addr::UNSPECIFIED)`, and help text describing the bind host.
- Pass the parsed host into `server_config` from both explicit and implicit server paths.
- Change `server_config` to accept `host: IpAddr` and assign `config.host = host`.
- Preserve the existing requirement that a trailing command is mandatory.
- Preserve `ServerConfig::new` as the server crate's default constructor with `0.0.0.0`; no server crate API change is required.

Modify `README.md` after implementation review to document:

- Server mode binds to `0.0.0.0` by default.
- `--host 127.0.0.1` restricts binding to localhost or another explicit IP literal.

## Tests

Add focused CLI parser coverage in `crates/terminald-cli/src/args.rs`:

- Explicit server mode without `--host` still yields `IpAddr::V4(Ipv4Addr::UNSPECIFIED)`.
- Implicit server mode without `--host` still yields `IpAddr::V4(Ipv4Addr::UNSPECIFIED)`.
- Explicit server mode with `--host 127.0.0.1` yields `IpAddr::V4(Ipv4Addr::LOCALHOST)`.
- Implicit server mode with `--host ::1` yields `IpAddr::V6(Ipv6Addr::LOCALHOST)`.
- Top-level help includes `--host <HOST>` and the bind-host help text.
- Server subcommand help includes `--host <HOST>` and the bind-host help text.

Expected focused verification:

```bash
cargo test -p terminald-cli args::tests
```

Expected final verification:

```bash
cargo fmt --check
cargo test -p terminald-cli
cargo test --workspace
```

## Acceptance Criteria

- `terminald server bash` and `terminald bash` still bind `0.0.0.0:<port>` by default through `ServerConfig.host`.
- `terminald server --host 127.0.0.1 bash` produces a server config whose host is `127.0.0.1`.
- `terminald --host ::1 bash` produces a server config whose host is `::1`.
- CLI help documents the host option for explicit and implicit server modes.
- Client mode behavior is unchanged.
- Source files under `crates/` remain below 400 LOC.

## Non-Goals

- Do not add hostname resolution or accept DNS names.
- Do not add environment-variable configuration.
- Do not change server bind error handling.
- Do not change client connection URL behavior.
- Do not change authentication, routes, frontend assets, or WebSocket protocol behavior.
