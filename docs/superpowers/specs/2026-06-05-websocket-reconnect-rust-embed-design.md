# WebSocket Reconnect And Rust Embed Assets Design

## Goal

Implement two milestones for Terminald:

1. The browser frontend automatically reconnects after an established WebSocket disconnects, following ttyd-style behavior where a dropped terminal session attempts to reconnect without requiring a page refresh.
2. The Rust server embeds a whole frontend asset directory with `rust-embed`, with a checked-in default `index.html` that displays `No frontend built` when a real frontend bundle has not been generated.

## Current Behavior

- `frontend/src/App.tsx` performs `auth/check`, opens one WebSocket, and changes the UI to `closed` after an established socket closes.
- Resize state is already retained in `latestSizeRef` and sent when a socket opens.
- `crates/terminald-server/src/assets.rs` manually embeds three named files with `include_bytes!`.
- `crates/terminald-server/assets` currently contains generated frontend files in this worktree, but `.gitignore` keeps generated assets ignored except `.gitkeep`.
- Runtime external asset override through `AssetConfig::with_external_dist` is already supported and should remain.

## Milestone 1: Frontend WebSocket Reconnect

### Requirements

- After a WebSocket has reached `open`, any later `close` event schedules a reconnect attempt instead of permanently showing `closed`.
- Reconnect attempts repeat until one succeeds, the React component unmounts, or an explicit authentication/authorization failure is observed.
- Each reconnect attempt repeats the existing `auth/check` request before opening a new WebSocket, preserving current Basic-auth behavior.
- On reconnect-time `auth/check` status `401`, stop retrying and show `authentication required`.
- On reconnect-time `auth/check` non-`204` status other than `401`, stop retrying and show `authentication check failed`.
- On reconnect-time network/fetch rejection, keep showing `reconnecting` and schedule another retry after the fixed reconnect delay.
- On initial `auth/check` network/fetch rejection, preserve current behavior and show the thrown error message or `connection error`; do not enter the reconnect loop.
- A WebSocket that closes before `open` during the initial connection reports `authentication or websocket upgrade failed` and does not enter the reconnect loop.
- A WebSocket that closes before `open` during a reconnect attempt keeps showing `reconnecting` and schedules another retry after the fixed reconnect delay.
- WebSocket `error` on an established socket should not permanently stop reconnect behavior; the browser usually follows it with `close`, and the close handler owns the retry scheduling.
- The UI status message should make reconnect progress visible. Use `reconnecting` while a retry is pending or in progress, then return to `connected` after the new socket opens.
- Use a fixed reconnect delay of 1000 ms between an established socket close and the next reconnect attempt.
- The latest terminal size must still be sent when the reconnected socket opens.
- Cleanup on unmount must close any active socket and cancel any pending reconnect timer.

### Acceptance Criteria

- Existing unaffected auth and connection tests still pass; tests that previously expected a permanent `closed` state after an established WebSocket close must be updated or replaced to expect `reconnecting`.
- A new frontend test proves an established socket close schedules a reconnect and creates a second WebSocket after timers advance.
- A new frontend test proves the latest size captured before reconnect is sent on the reconnected socket open.
- A new frontend test proves unmount cancels a pending reconnect and does not create another WebSocket.
- Immediate close before open remains an error and does not schedule a retry.
- A reconnect-attempt WebSocket that closes before open remains in `reconnecting` and schedules another retry.
- A reconnect-time `auth/check` `401` stops retrying and shows `authentication required`.
- A reconnect-time fetch/network rejection keeps showing `reconnecting` and schedules another retry.

## Milestone 2: Rust Embed Asset Directory

### Requirements

- Add `rust-embed` to the server crate and derive `RustEmbed` for the entire embedded frontend asset directory at `crates/terminald-server/assets`.
- Replace the manual `include_bytes!` asset match with lookup through the generated embed type.
- Keep `AssetConfig::with_external_dist` behavior: runtime files in an external dist directory take priority over embedded assets.
- Keep the tracked fallback page separate from generated frontend output:
  - `crates/terminald-server/assets/index.html` is the checked-in fallback page containing visible text `No frontend built`.
  - `npm --prefix frontend run build` writes generated frontend output to `crates/terminald-server/assets/dist/`.
  - `.gitignore` keeps `assets/dist/` generated output ignored while allowing `assets/index.html` to remain tracked.
  - The embedded asset loader first looks for generated embedded assets under `dist/{relative_path}` and falls back to `{relative_path}`. For example, `GET /` prefers `dist/index.html` when the frontend has been built before compiling, otherwise it serves `index.html`.
- Delete or exclude obsolete generated files outside `crates/terminald-server/assets/dist/`, especially `crates/terminald-server/assets/assets/*`, so the embed directory contains only the tracked fallback and ignored generated `dist/**` output.
- Continue preserving reverse-proxy path behavior:
  - extensionless app routes map to `index.html`;
  - paths containing `assets/` map from that segment onward;
  - unknown embedded paths return `404`;
  - suspicious request paths or relative paths containing a `..` segment return `404` and are rejected before route asset normalization, external filesystem lookup, and embedded lookup.
- Keep content types based on requested relative path.
- Do not check in generated JavaScript/CSS bundles as source assets. Built frontend files remain generated output under `assets/dist/`.

### Acceptance Criteria

- Server asset tests prove `GET /` returns the default embedded `No frontend built` HTML when no external dist is configured and no embedded `assets/dist/index.html` exists.
- Server asset tests prove generated embedded `assets/dist/index.html` takes precedence over fallback `assets/index.html` when present. Use a factored path-candidate/selection helper that can be unit-tested with injected available paths; do not require checked-in generated bundles or a prior frontend build for this test.
- Server asset tests prove external dist content still overrides embedded fallback assets.
- Server static asset prefix behavior is adjusted to match the default bundle: `GET /assets/terminald.css` may return `404` until the frontend has been built and copied into the embedded asset directory, while app routes still return embedded `index.html`.
- Asset traversal regression tests prove `AssetConfig::load` returns `None`, which routes as `404`, for suspicious paths such as `/..`, `/../index.html`, `/assets/..`, `/assets/../index.html`, and `/assets/../secret.txt`.
- Asset traversal regression tests prove suspicious paths are rejected before external filesystem lookup and cannot serve files outside or elsewhere inside the configured dist root through `..` segments.
- `cargo build --workspace` works without running `npm --prefix frontend run build`.
- `npm --prefix frontend run build` does not modify the tracked fallback `crates/terminald-server/assets/index.html`.
- Documentation explains that Rust builds include the default fallback page and that a frontend build is needed for the full UI.

## Non-Goals

- Do not add server-side session resume or terminal output replay.
- Do not preserve PTY state across reconnects; the server creates a fresh PTY for each accepted WebSocket.
- Do not add configurable reconnect backoff settings.
- Do not change CLI client reconnect behavior.
- Do not add a custom login page.

## Test And Verification Plan

- Frontend:
  - `npm --prefix frontend run test -- --run`
  - `npm --prefix frontend run lint`
  - `npm --prefix frontend run build`
- Rust:
  - `cargo fmt --check`
  - `cargo build --workspace`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo test --workspace`
- Source files under `crates/` and `frontend/src/` must remain below the README's 400 LOC limit. If added server asset tests would push `routes.rs` over that limit, move asset-loader unit tests into `assets.rs` instead of growing route tests.

## Documentation Impact

- Update `README.md` build notes to distinguish:
  - Rust-only builds embed the checked-in `No frontend built` fallback page.
  - Full web UI builds require `npm --prefix frontend run build`, which writes generated frontend output into `crates/terminald-server/assets/dist` for embedding.
- Update frontend behavior notes to mention automatic reconnect after disconnected WebSocket sessions.
