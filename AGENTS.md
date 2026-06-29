# AGENTS.md

## Project Overview

Terminald is a PTY-backed web terminal. The repository contains a Rust 2024 Cargo workspace, a Vite/React/TypeScript frontend, and a Nix flake for reproducible builds and formatting.

The CLI binary is named `terminald`. It can run an authenticated web terminal server or connect as a terminal client.

## Workspace Layout

- `Cargo.toml`: Cargo workspace manifest and shared dependency versions.
- `crates/terminald-protocol`: shared WebSocket message protocol types and encoding/decoding.
- `crates/terminald-pty`: Unix PTY process management.
- `crates/terminald-server`: Axum server, auth, static assets, routes, and PTY WebSocket sessions.
- `crates/terminald-cli`: CLI argument parsing and server/client entrypoint.
- `frontend`: Vite React frontend using `ghostty-web`.
- `crates/terminald-server/assets`: checked-in fallback `index.html` plus generated frontend assets during full UI builds.
- `flake.nix`: Nix build, dev shell, overlay, and treefmt configuration.

Source files under `crates/` and `frontend/src/` should stay below 400 LOC. Split modules before they grow past that point. Do not use `include!` to split Rust files.

## Build And Packaging

Preferred Nix commands:

```bash
nix build
nix flake check
nix fmt
```

`nix build` builds the frontend first, writes generated assets to `crates/terminald-server/assets/dist`, then builds the Rust binary with `crane` and the `fenix` stable Rust toolchain. The flake uses `flake-parts`, imports `flake-parts.flakeModules.easyOverlay`, and exposes `terminald` through `packages.default`, `packages.terminald`, and `overlays.default` via `overlayAttrs`.

Non-Nix build commands:

```bash
npm --prefix frontend install
npm --prefix frontend run build
cargo build --workspace
```

`cargo build --workspace` must continue to work without running the frontend build. The checked-in fallback page in `crates/terminald-server/assets/index.html` keeps Rust-only builds usable. Do not remove that fallback unless the project requirements explicitly change.

## Formatting

The flake configures `treefmt-nix` with:

- `nixfmt` RFC style for Nix files.
- `rustfmt` for Rust files.
- `prettier` for Markdown files.

Run `nix fmt` after touching Nix, Rust, or Markdown files. For Rust-only edits, also run `cargo fmt --check` when practical.

## Verification

After code changes, run the smallest relevant checks first, then the full checks needed for the changed area.

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

Nix:

```bash
nix fmt
nix flake check
nix build
```

If a required tool is unavailable, state that clearly and run the closest available verification.

## Error Handling Requirements

Do not discard lower-level exceptions or cause/context chains. Across runtime, reload, listener, configuration source, network, and system-call boundaries, preserve underlying error information for diagnosis.

When logging Rust `anyhow` errors, prefer full context-chain formatting such as `{err:#}` or `{error:#}` instead of only the outer `Display` message.

## Implementation Rules

- Keep changes scoped to the request.
- Avoid speculative features, compatibility shims, and premature abstractions.
- No legacy fallback behavior unless explicitly required.
- Validate only at real system boundaries, such as user input and external APIs.
- Do not add defensive checks inside trusted internal call paths without a concrete reason.
- Prefer idiomatic Rust 2024 and modern TypeScript/React patterns already used in the project.
- Remove unused code introduced by your own changes, but do not clean up unrelated code without being asked.
- Preserve generated frontend JS/CSS as ignored source artifacts; do not commit `crates/terminald-server/assets/dist` unless project policy changes.

## CLI Behavior Notes

- `terminald server ...` and implicit `terminald ...` run server mode.
- A trailing server command is required. Terminald must not fall back to `$SHELL` when no command is provided.
- `terminald client --connect <url>` connects to a server WebSocket endpoint.
- `terminald version` prints the package version.

## Frontend And Asset Rules

The frontend must use relative asset URLs and resolve `auth/check` plus `ws` from the current page path so deployments behind path prefixes continue to work.

The Vite build output is `crates/terminald-server/assets/dist`. `rust-embed` includes those generated assets when they exist; otherwise Rust-only builds rely on the checked-in fallback `index.html`.
