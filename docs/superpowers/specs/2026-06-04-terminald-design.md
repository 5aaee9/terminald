# Terminald Design

## Goal

Build `terminald`, a Rust web terminal similar to ttyd, with a React frontend under `/frontend`, Rust crates under `/crates`, and a `terminald` CLI that can run as a server or connect as a terminal client.

## Scope

The first complete version provides:

- A single PTY-backed web terminal session per WebSocket connection.
- Basic authentication compatible with `-c user:password`.
- A Vite/React/TypeScript frontend using the published `ghostty-web` package.
- A CLI with `server` and `client` modes.
- Build, lint, unit, and integration test coverage for the implemented behavior.

This version intentionally excludes multi-user session sharing, TLS termination, file transfer, terminal recording, and advanced ttyd options such as custom index files, reconnect replay buffers, or SSL certificate management. Reverse proxies are supported by resolving frontend assets and API endpoints from the browser's current normalized mount path instead of hard-coding `/`; the server accepts WebSocket upgrades at any path whose final path segment is `ws`.

## Architecture

Terminald is a Cargo workspace plus a nested frontend package:

- `crates/terminald-cli`: binary crate for clap parsing and process startup.
- `crates/terminald-server`: HTTP/WebSocket server, static frontend serving, authentication, and WebSocket-to-PTY session orchestration.
- `crates/terminald-pty`: Unix PTY spawning, resizing, reading, writing, and process cleanup using `nix::pty`.
- `crates/terminald-protocol`: shared WebSocket message framing and resize payload types.
- `frontend`: Vite + React + TypeScript + Vitest + ESLint application.

File sizes must stay below 400 LOC. Code is split by responsibility before files approach that limit.

## WebSocket Protocol

The browser and CLI client connect to `GET ws` relative to the current page URL after mount-path normalization. The server accepts WebSocket upgrades for `/ws`, `/aaa/ws`, `/example/bbb/ws`, and any other path ending in `/ws`. The server accepts WebSocket binary and text frames. Binary frames use a one-byte operation prefix:

| Prefix | Direction | Payload | Meaning |
| --- | --- | --- | --- |
| `0` | client to server | UTF-8 JSON `{ "cols": number, "rows": number }` | Resize PTY. |
| `1` | client to server | raw bytes | Write bytes to PTY stdin. |
| `2` | server to client | raw bytes | PTY stdout/stderr output. |
| `3` | server to client | UTF-8 text | Server-side error message before close. |

The server also accepts text frames from clients as stdin bytes to simplify testing and the CLI client. Invalid resize JSON is treated as a boundary error and reported to the client without panicking.

## Server Behavior

`terminald server -p 7681 -c f56a8193:fb604749b91b0110dad4adfb bash` starts an HTTP server on `0.0.0.0:7681` and runs `bash` for each authenticated WebSocket connection. `terminald -p 7681 -c ... bash` is equivalent to `terminald server ...`.

The PTY command is required for server mode. `terminald server`, `terminald -p 7681`, and other server invocations without a trailing command exit with a clap error explaining that a command is required. There is no implicit `$SHELL` or `sh` fallback.

Server requirements:

- Use `axum` and `tower` for HTTP routing and static asset serving.
- Use `tokio` for asynchronous networking and task orchestration.
- Serve `frontend/dist` assets from the workspace when that directory exists. This is the development override and takes priority over embedded assets.
- Embed a checked-in minimal frontend bundle under `crates/terminald-server/assets` so `cargo build --workspace` produces a usable binary even before `frontend/dist` exists. Release packaging may refresh that directory from `npm --prefix frontend run build`, but runtime behavior does not depend on running npm.
- Include an automated server asset test proving that `GET /` returns embedded `index.html` when no external asset directory is configured.
- Route unknown non-WebSocket `GET` paths to `index.html` so client-side routing and reverse-proxy paths work.
- Redirect extensionless non-WebSocket GET paths that do not end in `/` to the same path with a trailing slash. Examples: `/aaa` redirects to `/aaa/`; `/example/bbb` redirects to `/example/bbb/`. This gives browser relative URLs a stable mount path for assets, `auth/check`, and `ws`.
- Authenticate every HTTP and WebSocket route with HTTP Basic auth when `-c user:password` is configured, including `index.html`, static assets, `auth/check`, and every path ending in `/ws`.
- Return `401 Unauthorized` with `WWW-Authenticate: Basic realm="terminald"` for missing or invalid credentials on protected routes.
- Return `204 No Content` from any path ending in `/auth/check` when auth is disabled or when auth is enabled and the request has valid credentials. Return `401 Unauthorized` with the same challenge header for missing or invalid credentials.
- Require WebSocket upgrade requests to include valid Basic credentials when auth is enabled. The frontend relies on the preceding protected page and `auth/check` requests to populate browser credential cache for that origin and realm; if the WebSocket still fails with `401` or closes immediately, the frontend reports an authentication error instead of retrying indefinitely.
- Spawn a fresh PTY for each accepted WebSocket connection.
- Forward PTY output to WebSocket output frames and client input frames to PTY stdin.
- Apply resize messages with `ioctl(TIOCSWINSZ)` through `nix`.
- Preserve lower-level error context in `anyhow` errors and logs.
- Never log configured credentials, received Authorization headers, or decoded passwords. Credential comparison happens at the HTTP boundary and produces only pass/fail results.

## Client Behavior

`terminald client --connect http://127.0.0.1:7681 -c f56a8193:fb604749b91b0110dad4adfb` connects to the server WebSocket, attaches local stdin/stdout, and behaves like a simple remote terminal client.

Client requirements:

- Build the WebSocket URL by resolving `ws` relative to the provided HTTP(S) URL after ensuring the path ends in `/`.
- Send HTTP Basic auth when `-c` is configured.
- Put the local terminal in raw mode while connected.
- Relay stdin bytes to the WebSocket as input frames.
- Write output frames to stdout.
- Send an initial resize frame from the local terminal size and send another resize frame on `SIGWINCH` when supported.
- Restore the local terminal mode on exit.

## Frontend Behavior

The frontend starts as the terminal screen, not a marketing page. It uses `ghostty-web` for VT100 parsing/rendering and owns WebSocket connection state.

Frontend requirements:

- Use Vite, React, TypeScript, Vitest, and ESLint.
- Use relative asset paths with Vite `base: "./"` so built assets work from normalized mount paths such as `/aaa/` and `/example/bbb/`.
- Normalize browser URLs by relying on the server trailing-slash redirect for extensionless mount paths. After normalization, resolve the WebSocket endpoint with `new URL("ws", window.location.href)` and switch the protocol to `ws:` or `wss:`.
- Prompt the browser for HTTP Basic auth through protected page/static responses rather than building a custom login screen.
- Call `GET auth/check` before opening the WebSocket. A `204 No Content` response means auth is disabled or valid cached credentials exist. A `401` response means the browser challenge failed or was cancelled; the frontend shows an authentication error and does not open the WebSocket.
- Open the WebSocket only after the auth check returns `204 No Content`. If the WebSocket upgrade still fails authentication, show the closed/error state without leaking credentials in UI text or logs.
- Render connection states: connecting, connected, closed, and error.
- Forward keyboard input to WebSocket input frames.
- Send terminal resize frames when the terminal element changes size.
- Keep UI controls minimal and terminal-focused.

Because the exact `ghostty-web` public API can change, the implementation wraps it in a small adapter component. Tests cover endpoint resolution and protocol encoding independently of the rendering package.

## Testing And Verification

Rust verification:

- `cargo fmt --check`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo test --workspace`
- `cargo nextest run --manifest-path Cargo.toml --workspace` when `cargo-nextest` is available

Frontend verification:

- `npm --prefix frontend run lint`
- `npm --prefix frontend run test -- --run`
- `npm --prefix frontend run build`

Smoke verification:

- Start `terminald server -p <free-port> sh -lc 'printf ready; cat'`.
- Connect a WebSocket test client to `/ws`.
- Verify PTY output is received, input is echoed through `cat`, and resize messages are accepted.

## Documentation

The project README must document:

- What Terminald is.
- How to build the frontend and Rust binary.
- Server command examples, including default `server` behavior.
- Client command examples.
- Authentication format.
- Reverse-proxy deployment note for relative assets.
- Mount paths without a trailing slash are redirected to a trailing slash; reverse proxies should preserve that redirect or mount Terminald at a trailing-slash prefix.
- Basic auth is plaintext without TLS. README examples must state that `-c` should be used behind TLS termination or for trusted local networks only.
- Credentials and Authorization headers are never logged.
- Verification commands.

## Acceptance Criteria

- `cargo build --workspace` creates the `terminald` binary.
- `terminald server` and default `terminald` invocation parse the requested CLI shapes.
- `terminald client --connect ...` parses and attempts a WebSocket connection with optional credentials.
- The server serves frontend assets and upgrades `/ws`.
- The server redirects `/aaa` to `/aaa/`, serves the app at `/aaa/`, and upgrades `/aaa/ws`.
- `GET /auth/check` returns `204` without auth configured, `401` without credentials when auth is configured, and `204` with valid credentials.
- Authenticated WebSocket sessions spawn PTYs and bridge input/output.
- Frontend builds with relative asset URLs and uses a relative WebSocket URL.
- TypeScript lint/tests and Rust format/clippy/tests pass or any unavailable tool is reported precisely.
- No source file created for this implementation exceeds 400 LOC.
