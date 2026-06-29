# Structural And Reliability Optimizations Design

## Goal

Improve Terminald maintainability and PTY cleanup reliability in two focused phases without changing the public CLI, WebSocket protocol, frontend behavior, authentication behavior, or asset serving semantics.

## Scope

This optimization covers two implementation phases:

1. Split oversized server route code so source files under `crates/` remain below the project limit of 400 LOC and route responsibilities are easier to review.
2. Make PTY process cleanup consistently terminate the spawned process group, including children started by the shell or command, when a websocket session ends or a `PtyProcess` is dropped.

The work intentionally excludes new user-facing features, terminal session sharing, reconnect replay, TLS, custom login UI, configurable reconnect behavior, and protocol changes.

## Current Problems

- `crates/terminald-server/src/routes.rs` is over the repository's 400 LOC source-file limit. It mixes runtime dispatch helpers with a large test module covering auth, assets, redirects, and websocket behavior.
- `PtyProcess::terminate()` sends `SIGTERM` to the spawned process group, but `Drop` sends `SIGTERM` only to the child pid. A session dropped by websocket disconnect can leave grandchildren running when the command is a shell that starts child processes.

## Phase 1: Server Route Structure

### Design

Keep the production route behavior unchanged while moving route tests out of `routes.rs` into a dedicated test module file under `crates/terminald-server/src/routes/` or another idiomatic module split. The production route dispatch helpers stay small and focused.

The split must preserve existing test coverage for:

- `auth/check` behavior with and without Basic auth.
- Extensionless mount redirects.
- Embedded fallback index serving.
- Missing embedded static assets returning `404`.
- HTTP auth protection for app and asset routes.
- External dist overriding embedded fallback assets.
- WebSocket bridging, prefixed paths, Basic auth, text input, invalid resize error frames, and remote exit frames.

If moving tests requires exposing helper functions, expose only `pub(crate)` items needed by the internal test module. Do not change public crate API.

### Acceptance Criteria

- `crates/terminald-server/src/routes.rs` is below 400 LOC.
- Every Rust source file under `crates/` remains below 400 LOC.
- Existing route behavior and test coverage remain intact.
- No generated frontend assets are committed or intentionally modified.

## Phase 2: PTY Process Group Cleanup

### Design

Introduce a single cleanup path in `terminald-pty` that targets the spawned command's process group. `PtyProcess::terminate()` and `Drop` should use the same process-group termination intent where possible.

Expected semantics:

- The child setup already calls `setsid()`, making the spawned child the session leader and process group leader.
- Cleanup should check whether the direct child is still running before signaling where possible.
- Cleanup should send `SIGTERM` to `-child_pid` only while the process still owns an unreaped direct child, targeting the whole process group and avoiding signals after ownership has ended.
- `ESRCH`, an already-exited child, or an already-reaped child should be treated as non-fatal cleanup outcomes.
- Cleanup should avoid panicking from poisoned locks, missing children, or already-exited commands.
- Cleanup should still wait for the direct child to avoid zombies.
- Lower-level errors must retain context in fallible paths.

Because `Drop` cannot return errors, it may ignore best-effort termination/wait errors after attempting cleanup. Explicit `terminate()` must remain fallible and preserve context.

### Tests

Add or adjust focused Rust tests in `crates/terminald-pty/src/process.rs` to prove process-group cleanup. The regression test must exercise the `Drop` teardown path by dropping `PtyProcess`, not only by calling `terminate()`, because `terminate()` already targets the process group today. A practical test can spawn a shell command that starts a child process which writes a marker file from a signal trap or otherwise demonstrates that a process-group signal reached the child.

The test must be reliable enough for CI and bounded with timeouts. Avoid long sleeps. Use temporary directories and short polling windows. It must be constructed so it would fail against the current direct-pid `Drop` implementation and pass only when `Drop` targets the process group.

### Acceptance Criteria

- Dropping a `PtyProcess` sends termination to the process group rather than only the direct child pid.
- Explicitly terminating a `PtyProcess` continues to send termination to the process group.
- A regression test proves a child process started by the PTY command receives cleanup via the `Drop` process-group path and would fail against the current direct-pid `Drop` implementation.
- Existing PTY read/write/resize behavior remains intact.

## Documentation Impact

Update project documentation only if behavior visible to users changes. Since this is an internal reliability optimization with no new CLI or protocol behavior, README changes are not required unless implementation details expose a useful maintenance note.

## Verification And Evidence

Run the smallest relevant checks first, then full relevant workspace checks:

- `cargo fmt --check`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `cargo build --workspace`

If an optional tool such as `cargo nextest` is available, it may be run after the required checks.

Final implementation evidence must include:

- Line-count output proving `crates/terminald-server/src/routes.rs` and every Rust source file under `crates/` are below 400 LOC.
- Test output proving route behavior coverage still passes.
- Test output proving PTY read/write/resize behavior still passes.
- Test output proving a child process created by the PTY command receives process-group cleanup through the `Drop` path.
- A diff/status check proving generated frontend assets were not intentionally modified.

## Safety And Rollback

The route split is behavior-preserving and can be rolled back by moving tests back into the original file. The PTY cleanup change affects only session teardown. If it causes unexpected failures, rollback is limited to the cleanup helper and its tests without touching CLI, protocol, frontend, or route behavior.
