# Structural Reliability Optimizations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep server route code under the repository source-file limit and make PTY session teardown terminate the spawned process group through the Drop path.

**Architecture:** Split `crates/terminald-server/src/routes.rs` by moving its test module into `crates/terminald-server/src/routes/tests.rs` through a normal Rust `#[cfg(test)] mod tests;` submodule. Refactor `crates/terminald-pty/src/process.rs` so both explicit termination and Drop use a shared process-group cleanup helper that checks child state before signaling and waits for the direct child.

**Tech Stack:** Rust 2024, Cargo workspace, Axum, Tokio, nix/libc process primitives, tempfile for tests.

## Global Constraints

- Keep changes scoped to internal maintainability and PTY cleanup reliability.
- Do not change public CLI behavior, WebSocket protocol, frontend behavior, authentication behavior, or asset serving semantics.
- Source files under `crates/` and `frontend/src/` must stay below 400 LOC.
- Do not use `include!` to split Rust files.
- Preserve lower-level error context across runtime, listener, configuration, network, and system-call boundaries.
- Do not intentionally modify or commit generated frontend assets under `crates/terminald-server/assets/dist`.
- Run `cargo fmt --check`, `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, and `cargo build --workspace` before completion.

---

### Task 1: Split Server Route Tests

**Files:**
- Modify: `crates/terminald-server/src/routes.rs`
- Create: `crates/terminald-server/src/routes/tests.rs`

**Interfaces:**
- Consumes: Existing `app`, `auth_check`, `is_extensionless_without_trailing_slash`, and `AppState` items in `routes.rs`.
- Produces: A normal test submodule declared as `#[cfg(test)] mod tests;`; no public API changes.

- [ ] **Step 1: Inspect current route test block boundaries**

Run: `rg -n "#\[cfg\(test\)\]|mod tests" crates/terminald-server/src/routes.rs`

Expected: output shows the test module starts at the existing `#[cfg(test)] mod tests` block near the middle of `routes.rs`.

- [ ] **Step 2: Move the whole test module body into a new file**

Create `crates/terminald-server/src/routes/tests.rs` with the contents currently inside `mod tests { ... }`, excluding only the outer `#[cfg(test)] mod tests {` wrapper. The new file must start with these imports so the moved tests can access the parent module:

```rust
use super::*;

use axum::http::HeaderValue;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use futures_util::{SinkExt, StreamExt};
use http::header;
use http_body_util::BodyExt;
use tempfile::TempDir;
use terminald_protocol::{ClientMessage, Resize, ServerMessage};
use tokio::net::TcpListener;
use tokio_tungstenite::{
    connect_async,
    tungstenite::{Message as TungsteniteMessage, client::IntoClientRequest},
};
use tower::ServiceExt;

use crate::Credential;
```

- [ ] **Step 3: Replace the inline test module in `routes.rs`**

In `crates/terminald-server/src/routes.rs`, replace the entire old `#[cfg(test)] mod tests { ... }` block with:

```rust
#[cfg(test)]
mod tests;
```

Do not change production dispatch logic.

- [ ] **Step 4: Verify route tests still pass**

Run: `cargo test -p terminald-server routes::tests -- --nocapture`

Expected: all route tests pass, including auth, redirects, assets, and websocket behavior.

- [ ] **Step 5: Verify file line counts are now under the limit**

Run: `find crates -name '*.rs' -print0 | xargs -0 wc -l | sort -n`

Expected: `crates/terminald-server/src/routes.rs`, `crates/terminald-server/src/routes/tests.rs`, and every other Rust source file under `crates/` are below 400 LOC.

---

### Task 2: Add A Drop-Path PTY Process-Group Regression Test

**Files:**
- Modify: `crates/terminald-pty/Cargo.toml`
- Modify: `crates/terminald-pty/src/process.rs`

**Interfaces:**
- Consumes: Existing `PtyProcess::spawn`, `PtyCommand`, `PtySize`, async read/write helpers, and workspace `tempfile` dependency.
- Produces: A `tempfile.workspace = true` dev-dependency and a new test function `drop_terminates_process_group_children` that proves Drop signals a background child process started by the PTY command.

- [ ] **Step 1: Add `tempfile` as a test-only dependency**

In `crates/terminald-pty/Cargo.toml`, add this section after `[dependencies]`:

```toml
[dev-dependencies]
tempfile.workspace = true
```

- [ ] **Step 2: Add test helpers inside the existing `#[cfg(test)] mod tests` in `process.rs`**

Add these imports to the test module:

```rust
use std::path::Path;
```

Add these helper functions inside the test module:

```rust
async fn wait_for_file(path: &Path) -> bool {
    for _ in 0..50 {
        if tokio::fs::try_exists(path).await.unwrap() {
            return true;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}

async fn wait_until_contains(path: &Path, expected: &str) -> bool {
    for _ in 0..50 {
        if let Ok(contents) = tokio::fs::read_to_string(path).await {
            if contents.contains(expected) {
                return true;
            }
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    false
}
```

- [ ] **Step 3: Add a failing Drop-path regression test**

Add this test inside the same test module. The `trap` runs in the background subshell, not the direct shell child. The background subshell writes `ready` only after installing its `TERM` trap, so a direct-pid signal to the shell cannot satisfy the marker assertion.

```rust
#[tokio::test]
async fn drop_terminates_process_group_children() {
    let dir = tempfile::tempdir().unwrap();
    let ready = dir.path().join("ready");
    let trapped = dir.path().join("trapped");
    let script = format!(
        "( trap 'echo child-term > {trapped}; exit 0' TERM; touch {ready}; while true; do sleep 1; done ) & wait",
        ready = ready.display(),
        trapped = trapped.display(),
    );

    let process = PtyProcess::spawn(
        PtyCommand::new(vec!["sh".into(), "-lc".into(), script]),
        PtySize::default(),
    )
    .await
    .unwrap();

    assert!(wait_for_file(&ready).await, "background child process did not become ready");
    drop(process);

    assert!(
        wait_until_contains(&trapped, "child-term").await,
        "background child process did not receive SIGTERM from PTY Drop cleanup"
    );
}
```

Run: `cargo test -p terminald-pty drop_terminates_process_group_children -- --nocapture`

Expected before implementation: FAIL or timeout because the current Drop implementation signals only the direct shell pid, not the background child process group.

---

### Task 3: Implement Shared PTY Process-Group Cleanup

**Files:**
- Modify: `crates/terminald-pty/src/process.rs`

**Interfaces:**
- Consumes: Existing `PtyProcess`, `PtyHandle`, `Child`, `Pid`, `Signal`, `kill`, `Arc<Mutex<Child>>`, and `tokio::task` imports.
- Produces: Private shared cleanup helpers used by `PtyProcess::terminate()` and `Drop`, preserving existing public method signatures and avoiding any new public `PtyHandle` API.

- [ ] **Step 1: Add a helper to signal the process group**

Add this function near `is_pty_eof`:

```rust
fn terminate_process_group(child_id: u32) -> Result<()> {
    match kill(Pid::from_raw(-(child_id as i32)), Signal::SIGTERM) {
        Ok(()) | Err(Errno::ESRCH) => Ok(()),
        Err(error) => Err(error).with_context(|| format!("terminate PTY process group {child_id}")),
    }
}
```

- [ ] **Step 2: Add a single blocking cleanup helper that owns check/signal/wait under the child lock**

Add this private helper near `terminate_process_group`:

```rust
fn terminate_child_process_group_blocking(child: &mut Child) -> Result<()> {
    if child
        .try_wait()
        .context("check PTY child status before terminate")?
        .is_some()
    {
        return Ok(());
    }
    let child_id = child.id();
    terminate_process_group(child_id)?;
    child.wait().context("wait for PTY child after terminate")?;
    Ok(())
}
```

This keeps ownership coordination in one synchronous section: check that the direct child is still unreaped, signal its process group while the child mutex is held, then wait for the direct child before releasing ownership.

- [ ] **Step 3: Replace `PtyProcess::terminate()` with a private blocking cleanup call**

Change `PtyProcess::terminate()` to:

```rust
pub async fn terminate(&mut self) -> Result<()> {
    let child = Arc::clone(&self.inner.child);
    task::spawn_blocking(move || {
        let mut child = child
            .lock()
            .map_err(|_| anyhow!("PTY child lock poisoned"))?;
        terminate_child_process_group_blocking(&mut child)
    })
    .await
    .context("join PTY terminate task")?
}
```

This preserves the existing public `PtyProcess::terminate()` signature and does not add a new public method to `PtyHandle`.

- [ ] **Step 4: Add a synchronous best-effort cleanup method for Drop**

Add this private method to `impl PtyHandle`:

```rust
fn terminate_blocking_best_effort(&self) {
    let Ok(mut child) = self.child.lock() else {
        return;
    };
    let _ = terminate_child_process_group_blocking(&mut child);
}
```

- [ ] **Step 5: Replace the current Drop implementation body**

Change `impl Drop for PtyProcess` to:

```rust
impl Drop for PtyProcess {
    fn drop(&mut self) {
        self.inner.terminate_blocking_best_effort();
    }
}
```

- [ ] **Step 6: Run the targeted PTY regression test**

Run: `cargo test -p terminald-pty drop_terminates_process_group_children -- --nocapture`

Expected: PASS.

- [ ] **Step 7: Run existing PTY behavior test**

Run: `cargo test -p terminald-pty spawns_reads_writes_and_resizes -- --nocapture`

Expected: PASS.

---

### Task 4: Full Verification And Cleanup

**Files:**
- Inspect: `git status --short`
- Inspect: `git diff --stat`
- Inspect: `docs/superpowers/specs/2026-06-29-structural-reliability-optimizations-design.md`
- Inspect: `docs/superpowers/plans/2026-06-29-structural-reliability-optimizations.md`

**Interfaces:**
- Consumes: Completed Tasks 1-3.
- Produces: Verification evidence for final implementation review and commit readiness.

- [ ] **Step 1: Format Rust files**

Run: `cargo fmt`

Expected: command succeeds.

- [ ] **Step 2: Check formatting**

Run: `cargo fmt --check`

Expected: PASS.

- [ ] **Step 3: Run workspace tests**

Run: `cargo test --workspace`

Expected: PASS.

- [ ] **Step 4: Run clippy**

Run: `cargo clippy --workspace --all-targets -- -D warnings`

Expected: PASS.

- [ ] **Step 5: Build workspace**

Run: `cargo build --workspace`

Expected: PASS.

- [ ] **Step 6: Capture source line-count evidence**

Run: `find crates -name '*.rs' -print0 | xargs -0 wc -l | sort -n`

Expected: every Rust source file under `crates/` is below 400 LOC.

- [ ] **Step 7: Confirm generated frontend assets were not intentionally modified**

Run: `git status --short`

Expected: changed files are limited to the spec, plan, Rust source/test files, and any necessary docs. No generated frontend asset under `crates/terminald-server/assets/dist` should appear as modified or staged.

- [ ] **Step 8: Prepare final review evidence**

Run: `git diff -- crates/terminald-server/src/routes.rs crates/terminald-server/src/routes/tests.rs crates/terminald-pty/src/process.rs docs/superpowers/specs/2026-06-29-structural-reliability-optimizations-design.md docs/superpowers/plans/2026-06-29-structural-reliability-optimizations.md`

Expected: diff shows only the route test split, PTY cleanup/test changes, and SDD artifacts.
