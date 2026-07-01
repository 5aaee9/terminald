# Terminald

Terminald is a PTY-backed web terminal written in Rust, with a React frontend using `ghostty-web`.

## Build

```bash
nix build
```

The Nix flake builds the frontend first and embeds the generated assets into the Rust binary.

```bash
npm --prefix frontend install
npm --prefix frontend run build
cargo build --workspace
```

`cargo build --workspace` works without a frontend build; the server embeds a checked-in fallback page that says `No frontend built`.

Run `npm --prefix frontend run build` before release packaging or local full-UI testing. The frontend build writes generated assets into `crates/terminald-server/assets/dist`, where `rust-embed` includes them at compile time. Generated JS and CSS remain ignored source artifacts; the checked-in fallback `index.html` keeps Rust-only builds working and is not overwritten by the frontend build.

## Server

Run an authenticated server:

```bash
terminald server -p 7681 -c f56a8193:fb604749b91b0110dad4adfb bash
```

The default command is `server`, so this is equivalent:

```bash
terminald -p 7681 -c f56a8193:fb604749b91b0110dad4adfb bash
```

By default the server binds to `0.0.0.0`. Use `--host` with an IP literal to bind a specific interface, for example localhost only:

```bash
terminald server --host 127.0.0.1 -p 7681 bash
```

The trailing command is required. Terminald does not fall back to `$SHELL` when no command is provided.

The `-c` value is `username:password` and enables HTTP Basic authentication for the web app, static assets, `auth/check`, and WebSocket upgrade. Basic auth is plaintext without TLS; use it behind TLS termination or on trusted local networks only. Configured credentials and received `Authorization` headers are not logged or exposed through formatted auth errors.

## Client

Connect from a local terminal to a Terminald server:

```bash
terminald client --connect http://127.0.0.1:7681 -c f56a8193:fb604749b91b0110dad4adfb
```

The client normalizes the connect URL to the server's `ws` endpoint, sends Basic auth when `-c` is present, forwards stdin/stdout, sends the initial terminal size, and forwards resize updates on Unix.

## Reverse Proxy Paths

The frontend is built with relative asset URLs and resolves `auth/check` plus `ws` from the current page path. It works behind path prefixes such as:

```text
https://site.com/aaa/
https://site.com/example/bbb/
```

Use a trailing slash for mounted app paths. The server redirects extensionless non-WebSocket GET paths such as `/aaa`, `/example/bbb`, and `/custom` to the same path with a trailing slash.

The browser reconnects automatically after an established WebSocket disconnects. A reconnect starts a fresh PTY session; Terminald does not replay prior terminal output or resume the old process.

## Verification

Nix:

```bash
nix fmt
nix flake check
```

Rust:

```bash
cargo fmt --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
cargo nextest run --manifest-path Cargo.toml --workspace
```

Frontend:

```bash
npm --prefix frontend run lint
npm --prefix frontend run test -- --run
npm --prefix frontend run build
```

Source files under `crates/` and `frontend/src/` are kept below 400 LOC. Spec and plan files are excluded from that limit.
