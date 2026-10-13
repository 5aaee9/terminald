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

## Credential files

Both server modes and the client accept `--credential-file PATH` instead of `-c`:

```bash
terminald server --host 127.0.0.1 --credential-file /run/secrets/terminald-auth bash
terminald --credential-file /run/secrets/terminald-auth bash
terminald client --connect https://console.example/terminal/ --credential-file /run/secrets/terminald-auth
```

The file contains one UTF-8 `username:password` line. A single trailing LF or CRLF
is accepted; other whitespace and additional colons in the password are preserved.
Empty, malformed, multiline, NUL-containing, non-UTF-8, and unreadable files fail
closed. `--credential-file` conflicts with `-c` in every mode. The file is read once
at startup; changing it requires restarting the process.

This keeps the credential value out of process arguments. File ownership, access
permissions, secure delivery, and removal are the operator's responsibility.
Terminald does not create, chmod, delete, or log the file contents. The option works
with ordinary files, container secrets, or service-manager credential files; for
example, a systemd unit can use `LoadCredential=auth:/path/to/auth` and pass
`--credential-file %d/auth` in `ExecStart`.

## Unix socket listener

On Unix, `--unix-socket PATH` serves the same authenticated HTTP and WebSocket
endpoints over a filesystem socket instead of TCP:

```bash
# Create the parent directory with ownership appropriate for the service first.
umask 077
terminald server --unix-socket /run/terminald/http.sock --credential-file /run/secrets/terminald-auth bash
```

This is useful behind a local reverse proxy or across network namespaces using a
shared filesystem path. It conflicts with explicit `--host` and `--port` options;
the default TCP address and port do not create an additional listener. Relative
socket paths are resolved from the working directory.

Terminald neither creates parent directories nor unlinks existing paths, including
stale sockets and symlinks. Socket permissions follow the process umask. The
operator must restrict access to the parent directory/socket, grant the reverse
proxy access as needed, and clean up the socket after stopping the service. Basic
authentication is optional just as with TCP; a Unix socket does not imply that
connecting processes are trusted. The terminal client still connects via HTTP(S)
or WS(S), normally through the reverse proxy; `--unix-socket` is a server option.

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

Terminald does not compare browser `Origin` with the upstream `Host`, so a proxy
may rewrite Host without breaking authenticated WebSocket upgrades. Basic auth,
when configured, is still required for HTTP and WebSocket requests. Deployments
that need browser-origin restrictions must enforce their allowed origins at the
trusted reverse proxy. Do not expose an unauthenticated terminal to untrusted
networks or assume Basic auth alone prevents cross-origin browser requests.

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
