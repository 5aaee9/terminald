# Terminald Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a complete Rust/React web terminal with PTY-backed WebSocket sessions, authenticated server/client CLI modes, reverse-proxy-safe frontend assets, and verified docs.

**Architecture:** Implement a Cargo workspace split into protocol, PTY, server, and CLI crates, plus a Vite React frontend. The Rust server owns authentication, asset routing, path normalization, and PTY/WebSocket bridging; the frontend owns terminal rendering and protocol encoding.

**Tech Stack:** Rust, clap, anyhow, tokio, axum, tower-http, nix, portable-pty-compatible Unix PTY primitives through `nix::pty`, tokio-tungstenite, crossterm, React, Vite, TypeScript, Vitest, ESLint, `ghostty-web`.

---

## File Structure

- Create `Cargo.toml`: workspace members and shared dependency versions.
- Create `crates/terminald-protocol/src/{lib.rs,message.rs,resize.rs}`: frame prefixes, encode/decode helpers, resize JSON.
- Create `crates/terminald-pty/src/{lib.rs,process.rs,error.rs}`: Unix PTY process lifecycle, async read/write/resize API.
- Create `crates/terminald-server/src/{lib.rs,auth.rs,assets.rs,routes.rs,server.rs,session.rs}`: axum app, Basic auth, static/embedded assets, WebSocket session bridge.
- Create `crates/terminald-server/assets/{index.html,assets/terminald.css,assets/terminald.js}`: checked-in minimal embedded app bundle used when `frontend/dist` is absent.
- Create `crates/terminald-cli/src/{main.rs,args.rs,client.rs,server.rs,terminal.rs}`: clap CLI, default server command routing, terminal client.
- Create `frontend/{package.json,index.html,vite.config.ts,tsconfig*.json,eslint.config.js,vitest.setup.ts}`: Vite toolchain.
- Create `frontend/src/{main.tsx,App.tsx,styles.css}` and `frontend/src/terminal/{GhosttyTerminal.tsx,protocol.ts,urls.ts}`: browser app and tests.
- Create `README.md`: usage, build, auth, reverse-proxy, verification notes.

All implementation source files must remain below 400 LOC.

## Task 1: Workspace And Protocol Crate

**Files:**
- Create: `/home/indexyz/terminald/Cargo.toml`
- Create: `/home/indexyz/terminald/crates/terminald-protocol/Cargo.toml`
- Create: `/home/indexyz/terminald/crates/terminald-protocol/src/lib.rs`
- Create: `/home/indexyz/terminald/crates/terminald-protocol/src/message.rs`
- Create: `/home/indexyz/terminald/crates/terminald-protocol/src/resize.rs`

**Ownership:** Only the workspace manifest and `terminald-protocol` crate. Do not touch PTY, server, CLI, frontend, or README files in this task.

**Acceptance Criteria:**
- Workspace metadata resolves with `cargo metadata --no-deps`.
- `ClientMessage` decodes resize and input frames.
- `ServerMessage` encodes output and error frames.
- Resize JSON uses exact keys `cols` and `rows`.
- Invalid resize payloads return an error containing `invalid resize payload` and keep the serde cause in the error chain.

- [ ] **Step 1: Write protocol tests**

Create tests in `message.rs` and `resize.rs` covering:

```rust
assert_eq!(ClientMessage::decode(&[0, b'{', b'"', b'c']).unwrap_err().to_string(), "invalid resize payload");
assert_eq!(ClientMessage::decode(&[1, b'a']).unwrap(), ClientMessage::Input(vec![b'a']));
assert_eq!(ServerMessage::Output(vec![b'x']).encode(), vec![2, b'x']);
assert_eq!(Resize { cols: 80, rows: 24 }.to_payload().unwrap(), br#"{"cols":80,"rows":24}"#);
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `cargo test -p terminald-protocol`

Expected before implementation: package or symbol errors.

- [ ] **Step 3: Implement the workspace and protocol crate**

Create a workspace `Cargo.toml` with members under `crates/*`. Implement:

```rust
pub enum ClientMessage {
    Resize(Resize),
    Input(Vec<u8>),
}

pub enum ServerMessage {
    Output(Vec<u8>),
    Error(String),
}
```

Use prefix constants `0`, `1`, `2`, and `3`. Decode malformed resize payloads into a boundary error whose display includes `invalid resize payload`. Encode resize JSON with `serde_json`.

- [ ] **Step 4: Run protocol verification**

Run: `cargo test -p terminald-protocol`

Expected: all protocol tests pass.

Also run: `cargo metadata --no-deps --format-version 1`

Expected: workspace metadata exits `0` and lists `terminald-protocol`.

## Task 2: PTY Crate

**Files:**
- Create: `/home/indexyz/terminald/crates/terminald-pty/Cargo.toml`
- Create: `/home/indexyz/terminald/crates/terminald-pty/src/lib.rs`
- Create: `/home/indexyz/terminald/crates/terminald-pty/src/error.rs`
- Create: `/home/indexyz/terminald/crates/terminald-pty/src/process.rs`

**Ownership:** Only the `terminald-pty` crate and workspace dependency entries needed by this crate. Do not touch server, CLI, frontend, or docs in this task.

**Acceptance Criteria:**
- PTY allocation uses `nix::pty::openpty`.
- Child process setup preserves controlling terminal behavior with session/process-group setup.
- `PtyProcess::read`, `write_all`, `resize`, and `terminate` are callable from async code.
- Syscall failures include lower-level context through `anyhow::Context`.
- The integration test proves spawn, output read, stdin write, echoed output, resize, and cleanup.

- [ ] **Step 1: Write PTY integration tests**

Add tests that spawn:

```rust
PtyCommand::new(vec!["sh".into(), "-lc".into(), "printf ready; cat".into()])
```

Verify the reader emits `ready`, writing `hello\n` returns `hello`, and `resize(100, 40)` succeeds.

- [ ] **Step 2: Run the focused PTY tests and verify they fail**

Run: `cargo test -p terminald-pty`

Expected before implementation: missing type errors.

- [ ] **Step 3: Implement PTY process lifecycle**

Use `nix::pty::openpty`, `nix::unistd::{fork, setsid, dup2, execvp}`, `nix::sys::termios`, `nix::sys::wait`, and `tokio::task::spawn_blocking` where blocking file reads are needed. Expose:

```rust
pub struct PtyCommand { pub argv: Vec<String> }
pub struct PtyProcess { ... }

impl PtyProcess {
    pub async fn spawn(command: PtyCommand, size: PtySize) -> anyhow::Result<Self>;
    pub async fn read(&mut self, buf: &mut [u8]) -> anyhow::Result<usize>;
    pub async fn write_all(&mut self, data: &[u8]) -> anyhow::Result<()>;
    pub fn resize(&self, cols: u16, rows: u16) -> anyhow::Result<()>;
    pub async fn terminate(&mut self) -> anyhow::Result<()>;
}
```

Preserve syscall context with `anyhow::Context`.

- [ ] **Step 4: Run PTY verification**

Run: `cargo test -p terminald-pty`

Expected: PTY spawn, read/write, and resize tests pass on Linux.

Also run: `cargo test -p terminald-pty -- --nocapture`

Expected: no leaked child process diagnostics and no panic during cleanup.

## Task 3: Server Auth, Assets, And Routes

**Files:**
- Create: `/home/indexyz/terminald/crates/terminald-server/Cargo.toml`
- Create: `/home/indexyz/terminald/crates/terminald-server/src/lib.rs`
- Create: `/home/indexyz/terminald/crates/terminald-server/src/auth.rs`
- Create: `/home/indexyz/terminald/crates/terminald-server/src/assets.rs`
- Create: `/home/indexyz/terminald/crates/terminald-server/src/routes.rs`
- Create: `/home/indexyz/terminald/crates/terminald-server/src/server.rs`
- Create: `/home/indexyz/terminald/crates/terminald-server/assets/index.html`
- Create: `/home/indexyz/terminald/crates/terminald-server/assets/assets/terminald.css`
- Create: `/home/indexyz/terminald/crates/terminald-server/assets/assets/terminald.js`

**Ownership:** Server crate auth, route, asset, and app construction modules only. Do not implement WebSocket PTY bridging in this task.

**Acceptance Criteria:**
- `GET /auth/check` returns `204` when auth is disabled.
- `GET /auth/check` returns `401` plus `WWW-Authenticate: Basic realm="terminald"` when auth is enabled and credentials are missing or invalid.
- `GET /auth/check` returns `204` with valid Basic credentials.
- Prefixed `GET /aaa/auth/check` and `GET /example/bbb/auth/check` follow the same disabled, missing, invalid, and valid credential behavior as `/auth/check`.
- Extensionless non-WebSocket GET paths without a trailing slash redirect to the same path with `/`; examples include `/aaa`, `/example/bbb`, and `/custom`.
- `GET /aaa/` and `GET /example/bbb/` return embedded `index.html`.
- Unknown non-WebSocket GET paths with a trailing slash, such as `/foo/route/` and `/example/bbb/client/path/`, return `index.html` for client-side routing.
- `GET /assets/terminald.css` returns embedded CSS when no external asset directory is configured.
- `GET /aaa/assets/terminald.css` returns embedded CSS for prefixed mounts.
- When an external `frontend/dist` directory is configured and contains `index.html` or assets, those files are served before the embedded bundle.
- Protected static assets return `401` without credentials when auth is configured.
- Protected app routes return `401` without credentials when auth is configured, including `GET /`, `GET /aaa/`, `GET /foo/route/`, and `GET /example/bbb/client/path/`.
- Protected app routes return the expected `index.html` with valid credentials when auth is configured.
- Auth helpers never expose configured credentials, raw Authorization headers, or decoded passwords through `Debug`, `Display`, or error messages.

- [ ] **Step 1: Write server route tests**

Use `tower::ServiceExt` and `axum` request bodies to test:

```rust
GET /auth/check -> 204 when auth is disabled
GET /auth/check -> 401 with WWW-Authenticate when auth is enabled and no header
GET /auth/check -> 401 with WWW-Authenticate when Authorization is wrong
GET /auth/check -> 204 with Authorization: Basic base64(user:pass)
GET /aaa/auth/check -> 204 when auth is disabled
GET /aaa/auth/check -> 401 with WWW-Authenticate when auth is enabled and no header
GET /aaa/auth/check -> 401 with WWW-Authenticate when Authorization is wrong
GET /aaa/auth/check -> 204 with Authorization: Basic base64(user:pass)
GET /example/bbb/auth/check -> 204 when auth is disabled
GET /example/bbb/auth/check -> 401 with WWW-Authenticate when auth is enabled and no header
GET /example/bbb/auth/check -> 401 with WWW-Authenticate when Authorization is wrong
GET /example/bbb/auth/check -> 204 with Authorization: Basic base64(user:pass)
GET /aaa -> 308 or 301 redirect to /aaa/
GET /example/bbb -> 308 or 301 redirect to /example/bbb/
GET /custom -> 308 or 301 redirect to /custom/
GET /aaa/ -> embedded index.html
GET /example/bbb/ -> embedded index.html
GET /foo/route/ -> embedded index.html
GET /example/bbb/client/path/ -> embedded index.html
GET /assets/terminald.css -> embedded CSS
GET /aaa/assets/terminald.css -> embedded CSS
GET /assets/terminald.css -> 401 when auth is enabled and no header
GET / -> 401 when auth is enabled and no header
GET / -> embedded index.html when auth is enabled and Authorization is valid
GET /aaa/ -> 401 when auth is enabled and no header
GET /aaa/ -> embedded index.html when auth is enabled and Authorization is valid
GET /foo/route/ -> 401 when auth is enabled and no header
GET /foo/route/ -> embedded index.html when auth is enabled and Authorization is valid
GET /example/bbb/client/path/ -> 401 when auth is enabled and no header
GET /example/bbb/client/path/ -> embedded index.html when auth is enabled and Authorization is valid
```

Add a focused asset fallback test named `serves_embedded_index_without_external_dist` that constructs the app with no external asset directory and asserts `GET /` contains the embedded marker `terminald embedded app`.

Add a focused development override test named `serves_external_dist_before_embedded_assets` that creates a temporary `frontend/dist/index.html` containing `external dist app` and `frontend/dist/assets/terminald.css` containing `external css`, constructs the app with that directory, and asserts `GET /` and `GET /assets/terminald.css` return the external content instead of embedded markers.

Add a credential redaction test that formats auth errors and credential structs with `Debug` / `Display` where available and asserts the configured username, password, and raw `Authorization` header are not present.

- [ ] **Step 2: Run focused server route tests and verify they fail**

Run: `cargo test -p terminald-server auth assets routes`

Expected before implementation: missing crate or symbol errors.

- [ ] **Step 3: Implement auth and asset routing**

Implement `AuthConfig`, constant-time-enough byte equality for configured credentials, `WWW-Authenticate: Basic realm="terminald"`, no credential logging, embedded assets via `include_str!`, development `frontend/dist` override when present, trailing-slash redirects for extensionless non-WebSocket GET paths, and `auth/check` matching any path ending in `/auth/check`.

- [ ] **Step 4: Run server route verification**

Run: `cargo test -p terminald-server auth assets routes`

Expected: all route tests pass.

Also run: `cargo test -p terminald-server serves_embedded_index_without_external_dist credential`

Expected: embedded asset and credential redaction tests pass.

Also run: `cargo test -p terminald-server serves_external_dist_before_embedded_assets`

Expected: external `frontend/dist` assets take priority over embedded assets.

## Task 4: WebSocket PTY Sessions And Server Runner

**Files:**
- Modify: `/home/indexyz/terminald/crates/terminald-server/src/lib.rs`
- Modify: `/home/indexyz/terminald/crates/terminald-server/src/routes.rs`
- Modify: `/home/indexyz/terminald/crates/terminald-server/src/server.rs`
- Create: `/home/indexyz/terminald/crates/terminald-server/src/session.rs`

**Ownership:** Server crate WebSocket/session/runner code only. Do not change CLI or frontend files in this task.

**Acceptance Criteria:**
- Any path whose final segment is `ws` upgrades when auth is disabled.
- `/aaa/ws` upgrades and bridges PTY output/input.
- Auth-enabled WebSocket requests without Basic credentials are rejected before upgrade.
- Auth-enabled WebSocket requests with invalid Basic credentials are rejected before upgrade.
- Auth-enabled WebSocket requests with valid Basic credentials upgrade and bridge PTY I/O.
- Text WebSocket frames from clients are treated as UTF-8 stdin bytes and forwarded to the PTY.
- Invalid resize frames send an error frame and do not panic the server.
- Server runner binds the requested port and preserves bind/server error context.

- [ ] **Step 1: Write WebSocket smoke test**

Start the axum app on a local ephemeral listener with command `sh -lc 'printf ready; cat'`. Connect using `tokio_tungstenite`, read an output frame containing `ready`, send an input frame for `hello\n`, verify echoed output contains `hello`, send a resize frame, and close cleanly.

Add a text-frame stdin test that sends a WebSocket text frame containing `text hello\n` and verifies PTY output contains `text hello`.

Add matrix tests:

```rust
GET ws://host/ws without auth -> upgrades
GET ws://host/aaa/ws without auth -> upgrades
GET ws://host/ws with auth enabled and no header -> HTTP 401
GET ws://host/ws with auth enabled and wrong header -> HTTP 401
GET ws://host/aaa/ws with auth enabled and valid header -> upgrades
```

Add an invalid resize test that sends prefix `0` with malformed JSON and asserts the next server frame is prefix `3` with a non-secret error string.

- [ ] **Step 2: Run the WebSocket test and verify it fails**

Run: `cargo test -p terminald-server websocket`

Expected before implementation: missing WebSocket/session behavior.

- [ ] **Step 3: Implement session bridging**

Use `axum::extract::WebSocketUpgrade`. For accepted paths ending in `/ws`, authenticate before upgrade. Spawn `terminald_pty::PtyProcess`, split the WebSocket, forward PTY reads as `ServerMessage::Output`, forward binary client input frames to PTY stdin, forward text WebSocket frames as UTF-8 stdin bytes, apply resize messages, and send `ServerMessage::Error` for invalid boundary messages before closing.

- [ ] **Step 4: Implement server runner**

Expose:

```rust
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
    pub command: Vec<String>,
    pub credential: Option<Credential>,
}

pub async fn serve(config: ServerConfig) -> anyhow::Result<()>;
```

Bind with `tokio::net::TcpListener`, serve the axum app, and preserve bind/server errors with context.

- [ ] **Step 5: Run WebSocket verification**

Run: `cargo test -p terminald-server websocket`

Expected: smoke test passes.

Also run: `cargo test -p terminald-server websocket_auth websocket_invalid_resize`

Expected: WebSocket auth matrix and invalid resize behavior pass.

Also run: `cargo test -p terminald-server websocket_text_input`

Expected: text WebSocket frames are forwarded to PTY stdin.

## Task 5: CLI Server And Client

**Files:**
- Create: `/home/indexyz/terminald/crates/terminald-cli/Cargo.toml`
- Create: `/home/indexyz/terminald/crates/terminald-cli/src/main.rs`
- Create: `/home/indexyz/terminald/crates/terminald-cli/src/args.rs`
- Create: `/home/indexyz/terminald/crates/terminald-cli/src/server.rs`
- Create: `/home/indexyz/terminald/crates/terminald-cli/src/client.rs`
- Create: `/home/indexyz/terminald/crates/terminald-cli/src/terminal.rs`

**Ownership:** CLI crate only, plus workspace dependency entries needed by this crate.

**Acceptance Criteria:**
- Explicit `terminald server ...` and implicit `terminald ...` both select server mode.
- `cargo build --workspace` produces a binary named `terminald`; use `[[bin]] name = "terminald"` in the CLI crate if needed.
- Server mode requires a trailing command and emits a clap error containing `command is required` when missing.
- `-p` defaults to `7681` and accepts user-provided ports.
- Server mode binds to host `0.0.0.0` by default while preserving the requested port.
- `-c user:password` parses into the shared credential type without logging or printing the password.
- Client mode resolves `ws` URLs after trailing-slash normalization and attaches Basic auth when provided.
- Client mode sends an initial resize frame using the local terminal size before relaying stdin.
- Client mode sends another resize frame on `SIGWINCH` when the platform supports resize signals.
- Client mode restores terminal mode on normal WebSocket close, I/O error, and interrupted shutdown.

- [ ] **Step 1: Write CLI parsing tests**

Test `Cli::try_parse_from` for:

```rust
terminald server -p 7681 -c user:pass bash
terminald -p 7681 -c user:pass bash
terminald client --connect http://127.0.0.1:7681 -c user:pass
terminald server
```

Assert the first two parse as server mode with command `bash`, client parses the connect URL, and `terminald server` returns an error containing `command is required`.

Assert parsed server config for `terminald server -p 7681 bash` uses host `0.0.0.0` and port `7681`; assert `terminald server -p 9000 bash` preserves port `9000`.

Add a manifest/build assertion that the CLI crate declares a binary named `terminald` and that `cargo build -p terminald-cli --bin terminald` succeeds.

Add client runtime tests with injectable terminal and WebSocket abstractions:

```rust
initial terminal size 100x40 -> first sent frame decodes as ClientMessage::Resize(Resize { cols: 100, rows: 40 })
simulated SIGWINCH changing size to 120x50 -> next sent frame decodes as ClientMessage::Resize(Resize { cols: 120, rows: 50 })
normal close -> terminal raw mode guard restore is called
I/O error -> terminal raw mode guard restore is called
```

- [ ] **Step 2: Run focused CLI tests and verify they fail**

Run: `cargo test -p terminald-cli args`

Expected before implementation: missing crate or parser errors.

- [ ] **Step 3: Implement clap parsing and server entry**

Use clap derive. Treat an explicit `server` subcommand and a no-subcommand invocation as server mode. Require at least one trailing command. Parse `-c user:password` into shared credential type. Default port is `7681`.

- [ ] **Step 4: Implement terminal client**

Resolve `ws` relative to `--connect` after ensuring a trailing slash, convert `http` to `ws` and `https` to `wss`, send Basic auth header when `-c` is provided, put local terminal in raw mode with crossterm, send an initial resize frame from `crossterm::terminal::size`, relay stdin/stdout through protocol frames, listen for `SIGWINCH` with `tokio::signal::unix::signal(SignalKind::window_change())` on Unix and send updated resize frames when supported, and restore terminal mode on every exit path through a guard.

- [ ] **Step 5: Run CLI verification**

Run: `cargo test -p terminald-cli args`

Expected: CLI parser tests pass.

Also run: `cargo test -p terminald-cli client_url`

Expected: client URL normalization and auth header tests pass.

Also run: `cargo test -p terminald-cli client_resize terminal_restore`

Expected: initial resize frame, SIGWINCH resize frame, and terminal restore tests pass.

Also run: `cargo build -p terminald-cli --bin terminald`

Expected: Cargo builds a binary named `terminald`.

## Task 6: Frontend Toolchain And Protocol Tests

**Files:**
- Create: `/home/indexyz/terminald/frontend/package.json`
- Create: `/home/indexyz/terminald/frontend/index.html`
- Create: `/home/indexyz/terminald/frontend/vite.config.ts`
- Create: `/home/indexyz/terminald/frontend/tsconfig.json`
- Create: `/home/indexyz/terminald/frontend/tsconfig.node.json`
- Create: `/home/indexyz/terminald/frontend/eslint.config.js`
- Create: `/home/indexyz/terminald/frontend/vitest.setup.ts`
- Create: `/home/indexyz/terminald/frontend/src/terminal/protocol.ts`
- Create: `/home/indexyz/terminald/frontend/src/terminal/protocol.test.ts`
- Create: `/home/indexyz/terminald/frontend/src/terminal/urls.ts`
- Create: `/home/indexyz/terminald/frontend/src/terminal/urls.test.ts`

**Ownership:** Frontend package metadata and helper modules/tests only. Do not implement React terminal UI in this task.

**Acceptance Criteria:**
- `npm --prefix frontend install` creates a lockfile.
- Vite config uses `base: "./"`.
- Protocol helpers encode prefixes `0`, `1`, `2`, and `3` consistently with the Rust crate.
- URL helpers resolve from trailing-slash mount paths without hard-coding `/`.
- Tests prove `/aaa/` and `/example/bbb/` URL behavior.

- [ ] **Step 1: Write frontend protocol and URL tests**

Test:

```ts
encodeInput("a") returns Uint8Array [1, 97]
encodeResize(80, 24) starts with prefix 0 and JSON {"cols":80,"rows":24}
decodeOutput([2, 120]) returns Uint8Array [120]
resolveWebSocketUrl("http://site.com/aaa/") returns "ws://site.com/aaa/ws"
resolveAuthCheckUrl("https://site.com/example/bbb/") returns "https://site.com/example/bbb/auth/check"
```

- [ ] **Step 2: Run focused frontend tests and verify they fail**

Run: `npm --prefix frontend run test -- --run src/terminal`

Expected before implementation: missing package or test errors.

- [ ] **Step 3: Implement Vite toolchain and helpers**

Use `ghostty-web@0.4.0`, React, TypeScript, Vitest, ESLint. Set Vite `base: "./"`. Implement protocol encoding with `TextEncoder` and URL resolution with `new URL("ws", pageUrl)` / `new URL("auth/check", pageUrl)`.

- [ ] **Step 4: Run frontend helper verification**

Run: `npm --prefix frontend install`

Run: `npm --prefix frontend run test -- --run src/terminal`

Expected: helper tests pass.

Also run: `npm --prefix frontend run lint`

Expected: ESLint passes for helper files.

## Task 7: Frontend Terminal App

**Files:**
- Create: `/home/indexyz/terminald/frontend/src/main.tsx`
- Create: `/home/indexyz/terminald/frontend/src/App.tsx`
- Create: `/home/indexyz/terminald/frontend/src/styles.css`
- Create: `/home/indexyz/terminald/frontend/src/terminal/GhosttyTerminal.tsx`
- Create: `/home/indexyz/terminald/frontend/src/terminal/GhosttyTerminal.test.tsx`

**Ownership:** Frontend React app and terminal adapter only. Do not edit Rust crates in this task.

**Acceptance Criteria:**
- App starts on a terminal surface, not a landing page.
- `GET auth/check` is called before opening WebSocket.
- WebSocket opens only after `204`.
- `401` from auth check renders authentication error state and does not open WebSocket.
- UI renders connecting, connected, closed, and generic error states.
- If `auth/check` returns `204` but the WebSocket upgrade fails or closes immediately with an auth-like failure, the UI renders an authentication/error state instead of retrying indefinitely.
- `ghostty-web` is initialized before `Terminal` construction.
- Terminal input is sent as protocol input frames.
- Output frames are decoded and written to the terminal.
- ResizeObserver-driven size changes send resize frames.
- Built asset URLs are relative.

- [ ] **Step 1: Write app tests**

Mock `ghostty-web` and `WebSocket`. Verify the app calls `auth/check`, opens the WebSocket after `204`, writes output frames to the terminal adapter, sends input frames from `onData`, and renders an authentication error when auth check returns `401`.

Add connection-state tests:

```text
before auth/check resolves -> connecting state is visible
WebSocket open event after auth/check 204 -> connected state is visible
WebSocket close after open -> closed state is visible
WebSocket error after auth/check 204 -> error state is visible
WebSocket closes immediately after auth/check 204 -> authentication/error state is visible and no reconnect loop starts
```

- [ ] **Step 2: Run focused app tests and verify they fail**

Run: `npm --prefix frontend run test -- --run src/App`

Expected before implementation: missing component errors.

- [ ] **Step 3: Implement terminal app**

Use `init`, `Terminal`, and `FitAddon` from `ghostty-web`. Mount a full-viewport terminal surface, call `term.onData`, send protocol input frames, decode output frames into `term.write`, observe size changes with `ResizeObserver`, and send resize frames based on terminal cols/rows. Render compact connection status text only.

- [ ] **Step 4: Run frontend app verification**

Run: `npm --prefix frontend run test -- --run`

Run: `npm --prefix frontend run lint`

Run: `npm --prefix frontend run build`

Expected: tests, lint, and build pass.

Also inspect `frontend/dist/index.html`.

Expected: script and stylesheet references are relative paths beginning with `./` or `assets/`, not `/assets/`.

## Task 8: README, Full Verification, And Source Size Check

**Files:**
- Create: `/home/indexyz/terminald/README.md`
- Modify: `/home/indexyz/terminald/.gitignore`

**Ownership:** README, ignore files, and final verification only. Do not add new product features in this task.

**Acceptance Criteria:**
- README includes server and client command examples from the spec.
- README states missing server command is an error and no shell fallback exists.
- README documents reverse-proxy trailing-slash behavior.
- README warns Basic auth is plaintext without TLS and should be used behind TLS termination or trusted local networks only.
- README states configured credentials and received Authorization headers are never logged.
- README lists Rust and frontend verification commands.
- Final verification commands run freshly after all implementation changes.
- Final smoke verification starts the built `terminald` server binary, connects a WebSocket client, verifies PTY output/input echo, sends a resize frame, and shuts the server down.

- [ ] **Step 1: Write README**

Document project purpose, build commands, server examples, default server invocation, client example, Basic auth format, TLS warning, reverse-proxy trailing-slash behavior, relative asset behavior, the guarantee that credentials and Authorization headers are not logged, and verification commands.

- [ ] **Step 2: Run Rust verification**

Run:

```bash
cargo fmt --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

Expected for `cargo build --workspace`: exits `0` and creates the `terminald` binary through the CLI crate.

If `cargo nextest` exists, run:

```bash
cargo nextest run --manifest-path Cargo.toml --workspace
```

Expected: commands pass, or unavailable `cargo nextest` is reported exactly.

- [ ] **Step 3: Run frontend verification**

Run:

```bash
npm --prefix frontend run lint
npm --prefix frontend run test -- --run
npm --prefix frontend run build
```

Expected: commands pass.

- [ ] **Step 4: Run source size check**

Run:

```bash
find crates frontend/src -type f \( -name '*.rs' -o -name '*.ts' -o -name '*.tsx' \) -print0 | xargs -0 wc -l | awk '$1 > 400 { print }'
```

Expected: no files printed except the final `total` line if present.

- [ ] **Step 5: Run built-binary smoke verification**

Run the built binary on a free local port:

```bash
target/debug/terminald server -p <free-port> sh -lc 'printf ready; cat'
```

Then connect a WebSocket test client to `ws://127.0.0.1:<free-port>/ws` and verify:

```text
received output frame contains ready
sent input frame for "hello\n" produces output containing hello
sent resize frame {"cols":100,"rows":40} is accepted without an error frame
```

Expected: smoke client observes output/input echo and no resize error. Stop the server process after the smoke check.

- [ ] **Step 6: Inspect final diff**

Run:

```bash
git status --short
git diff --stat
```

Expected: changes are limited to Terminald implementation, docs, package lockfiles, and SDD artifacts.
