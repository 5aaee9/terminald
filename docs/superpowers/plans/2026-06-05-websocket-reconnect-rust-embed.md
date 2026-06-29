# WebSocket Reconnect And Rust Embed Assets Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add automatic browser WebSocket reconnects and replace manual embedded assets with `rust-embed` over a checked-in fallback frontend directory.

**Architecture:** Keep the existing React app and Rust server boundaries. The frontend owns a small reconnect loop inside `App.tsx`; the server keeps `AssetConfig` and route behavior but resolves fallback files through a `RustEmbed` directory type instead of hard-coded `include_bytes!` matches.

**Tech Stack:** React 19, Vitest, Vite, Rust 2024, axum, tokio, rust-embed.

---

## File Structure

- Modify `/home/indexyz/terminald/frontend/src/App.tsx`: reconnect loop, timer cleanup, status messages.
- Modify `/home/indexyz/terminald/frontend/src/App.test.tsx`: fake timers and reconnect behavior coverage.
- Modify `/home/indexyz/terminald/frontend/vite.config.ts`: write generated frontend build output to the ignored embedded `dist` subdirectory.
- Modify `/home/indexyz/terminald/Cargo.toml`: add workspace `rust-embed` dependency.
- Modify `/home/indexyz/terminald/crates/terminald-server/Cargo.toml`: depend on workspace `rust-embed`.
- Modify `/home/indexyz/terminald/crates/terminald-server/src/assets.rs`: derive `RustEmbed`, prefer embedded generated `dist` files, fall back to checked-in fallback files, keep external override.
- Modify `/home/indexyz/terminald/crates/terminald-server/src/assets.rs`: add asset-loader traversal unit tests; keep traversal coverage out of `routes.rs` because it is already near 400 LOC.
- Modify `/home/indexyz/terminald/crates/terminald-server/src/routes.rs`: update route-level asset tests for fallback HTML and CSS 404 behavior while keeping the file under 400 LOC.
- Modify `/home/indexyz/terminald/.gitignore`: keep fallback `index.html` tracked while generated bundle output stays ignored.
- Modify `/home/indexyz/terminald/crates/terminald-server/assets/index.html`: checked-in default `No frontend built` page.
- Delete `/home/indexyz/terminald/crates/terminald-server/assets/.gitkeep`: fallback `index.html` now keeps the directory present.
- Delete obsolete generated files outside `assets/dist`: `/home/indexyz/terminald/crates/terminald-server/assets/assets/terminald.css` and `/home/indexyz/terminald/crates/terminald-server/assets/assets/terminald.js`.
- Modify `/home/indexyz/terminald/README.md`: document fallback embed and automatic reconnect.

## Task 1: Frontend Reconnect Tests

**Files:**

- Modify: `/home/indexyz/terminald/frontend/src/App.test.tsx`

**Ownership:** Frontend tests only.

- [ ] **Step 1: Add timer helper and close-state assertions**

Update imports:

```ts
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
```

Add a helper near `decodeResize`; use it only in tests that advance reconnect timers:

```ts
async function advanceReconnectDelay() {
  await act(async () => {
    vi.advanceTimersByTime(1000);
  });
  await Promise.resolve();
}
```

Do not enable fake timers globally in `beforeEach`. In each reconnect-specific test that advances timers, call `vi.useFakeTimers()` at the start of the test. Add `vi.useRealTimers()` to the existing `afterEach` before `vi.unstubAllGlobals()`:

```ts
vi.useRealTimers();
```

Update the existing `renders closed and error states` test to expect established close to show `reconnecting`, and keep websocket error coverage limited to a pre-open initial socket:

```ts
it("renders reconnecting and error states", async () => {
  render(<App />);
  expect(screen.getByRole("status")).toHaveTextContent("connecting");
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  sockets[0].closeEvent();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("reconnecting");
  });

  cleanup();
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(2));
  sockets[1].fail();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("websocket error");
  });
});
```

- [ ] **Step 2: Add reconnect scheduling test**

Add this test:

```ts
it("reconnects after an established websocket closes", async () => {
  const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
  vi.stubGlobal("fetch", fetchMock);
  vi.useFakeTimers();
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  expect(fetchMock).toHaveBeenCalledTimes(1);
  sockets[0].open();

  sockets[0].closeEvent();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("reconnecting");
  });

  await advanceReconnectDelay();

  await waitFor(() => expect(sockets).toHaveLength(2));
  expect(fetchMock).toHaveBeenCalledTimes(2);
  sockets[1].open();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("connected");
  });
});
```

- [ ] **Step 3: Add resize replay test for reconnect**

Add this test:

```ts
it("sends the latest terminal size when a reconnect opens", async () => {
  vi.useFakeTimers();
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  screen.getByRole("button", { name: "terminal" }).click();
  expect(decodeResize(sockets[0].sent[1])).toEqual({ cols: 80, rows: 24 });

  sockets[0].closeEvent();
  await advanceReconnectDelay();

  await waitFor(() => expect(sockets).toHaveLength(2));
  sockets[1].open();
  await waitFor(() => expect(sockets[1].sent).toHaveLength(1));
  expect((sockets[1].sent[0] as Uint8Array)[0]).toBe(0);
  expect(decodeResize(sockets[1].sent[0])).toEqual({ cols: 80, rows: 24 });
});
```

- [ ] **Step 4: Add unmount cancellation and immediate-close no-retry tests**

Add these tests:

```ts
it("cancels pending reconnect when unmounted", async () => {
  vi.useFakeTimers();
  const view = render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  sockets[0].closeEvent();

  view.unmount();
  await advanceReconnectDelay();

  expect(sockets).toHaveLength(1);
});

it("does not retry an immediate websocket close before open", async () => {
  vi.useFakeTimers();
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].closeEvent();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("authentication or websocket upgrade failed");
  });

  await advanceReconnectDelay();

  expect(sockets).toHaveLength(1);
});

it("keeps retrying when a reconnect websocket closes before open", async () => {
  vi.useFakeTimers();
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  sockets[0].closeEvent();

  await advanceReconnectDelay();

  await waitFor(() => expect(sockets).toHaveLength(2));
  sockets[1].closeEvent();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("reconnecting");
  });

  await advanceReconnectDelay();

  await waitFor(() => expect(sockets).toHaveLength(3));
});

it("reconnects after an established websocket error is followed by close", async () => {
  vi.useFakeTimers();
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  sockets[0].fail();
  sockets[0].closeEvent();

  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("reconnecting");
  });
  await advanceReconnectDelay();

  await waitFor(() => expect(sockets).toHaveLength(2));
});
```

- [ ] **Step 5: Add reconnect auth and fetch failure tests**

Add these tests:

```ts
it("stops reconnecting when auth is rejected during retry", async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
  vi.stubGlobal("fetch", fetchMock);
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  fetchMock.mockResolvedValueOnce(new Response(null, { status: 401 }));

  sockets[0].closeEvent();
  await advanceReconnectDelay();

  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("authentication required");
  });
  expect(sockets).toHaveLength(1);
});

it("keeps retrying when auth check fetch fails during reconnect", async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
  vi.stubGlobal("fetch", fetchMock);
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  fetchMock.mockRejectedValueOnce(new Error("network down"));

  sockets[0].closeEvent();
  await advanceReconnectDelay();
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("reconnecting");
  });

  await advanceReconnectDelay();

  await waitFor(() => expect(sockets).toHaveLength(2));
});
```

- [ ] **Step 6: Add reconnect non-204 auth failure test**

Add this test:

```ts
it("stops reconnecting when auth check fails during retry", async () => {
  vi.useFakeTimers();
  const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
  vi.stubGlobal("fetch", fetchMock);
  render(<App />);
  await waitFor(() => expect(sockets).toHaveLength(1));
  sockets[0].open();
  fetchMock.mockResolvedValueOnce(new Response(null, { status: 500 }));

  sockets[0].closeEvent();
  await advanceReconnectDelay();

  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("authentication check failed");
  });
  expect(sockets).toHaveLength(1);

  await advanceReconnectDelay();
  expect(sockets).toHaveLength(1);
});
```

- [ ] **Step 7: Add initial auth fetch failure test**

Add this test:

```ts
it("reports initial auth check fetch failure without retrying", async () => {
  vi.useFakeTimers();
  vi.stubGlobal("fetch", vi.fn(async () => {
    throw new Error("network down");
  }));

  render(<App />);
  await waitFor(() => {
    expect(screen.getByRole("status")).toHaveTextContent("network down");
  });

  await advanceReconnectDelay();
  expect(sockets).toHaveLength(0);
});
```

- [ ] **Step 8: Run focused frontend tests and confirm failure**

Run:

```bash
npm --prefix frontend run test -- --run src/App.test.tsx
```

Expected before implementation: at least the reconnect tests fail because no second WebSocket is created and close still reports `closed`.

## Task 2: Frontend Reconnect Implementation

**Files:**

- Modify: `/home/indexyz/terminald/frontend/src/App.tsx`

**Ownership:** Frontend app component only.

- [ ] **Step 1: Add reconnect status and timer refs**

Change status type:

```ts
type Status = "connecting" | "connected" | "reconnecting" | "closed" | "error";
```

Add refs inside `App`:

```ts
const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
```

Add this module-level constant above `export default function App()`:

```ts
const RECONNECT_DELAY_MS = 1000;
```

- [ ] **Step 2: Replace the single-shot effect with a retrying connection loop**

Inside `useEffect`, replace the `cancelled/opened/connect` logic with a loop shaped like this:

```ts
useEffect(() => {
  let cancelled = false;
  let attempt = 0;

  const clearReconnectTimer = () => {
    if (reconnectTimerRef.current) {
      clearTimeout(reconnectTimerRef.current);
      reconnectTimerRef.current = null;
    }
  };

  const scheduleReconnect = () => {
    if (cancelled || reconnectTimerRef.current) {
      return;
    }
    setStatus("reconnecting");
    setMessage("reconnecting");
    reconnectTimerRef.current = setTimeout(() => {
      reconnectTimerRef.current = null;
      void connect(true);
    }, RECONNECT_DELAY_MS);
  };

  const scheduleReconnectAfterFailure = () => {
    if (cancelled || reconnectTimerRef.current) {
      return;
    }
    reconnectTimerRef.current = setTimeout(() => {
      reconnectTimerRef.current = null;
      void connect(true);
    }, RECONNECT_DELAY_MS);
  };

  async function connect(isReconnect: boolean) {
    const currentAttempt = ++attempt;
    setStatus(isReconnect ? "reconnecting" : "connecting");
    setMessage(isReconnect ? "reconnecting" : "connecting");

    let auth: Response;
    try {
      auth = await fetch(resolveAuthCheckUrl(), { credentials: "same-origin" });
    } catch (error) {
      if (cancelled || currentAttempt !== attempt) {
        return;
      }
      if (isReconnect) {
        setStatus("reconnecting");
        setMessage("reconnecting");
        scheduleReconnectAfterFailure();
      } else {
        setStatus("error");
        setMessage(error instanceof Error ? error.message : "connection error");
      }
      return;
    }
    if (cancelled || currentAttempt !== attempt) {
      return;
    }
    if (auth.status === 401) {
      setStatus("error");
      setMessage("authentication required");
      return;
    }
    if (auth.status !== 204) {
      setStatus("error");
      setMessage("authentication check failed");
      return;
    }

    const socket = new WebSocket(resolveWebSocketUrl());
    socket.binaryType = "arraybuffer";
    socketRef.current = socket;
    let opened = false;

    socket.addEventListener("open", () => {
      if (cancelled || currentAttempt !== attempt) {
        socket.close();
        return;
      }
      opened = true;
      setStatus("connected");
      setMessage("connected");
      const latestSize = latestSizeRef.current;
      if (latestSize) {
        socket.send(encodeResize(latestSize.cols, latestSize.rows));
      }
    });

    socket.addEventListener("close", () => {
      if (socketRef.current === socket) {
        socketRef.current = null;
      }
      if (cancelled || currentAttempt !== attempt) {
        return;
      }
      if (opened) {
        scheduleReconnect();
      } else if (isReconnect) {
        scheduleReconnect();
      } else {
        setStatus("error");
        setMessage("authentication or websocket upgrade failed");
      }
    });

    socket.addEventListener("error", () => {
      if (cancelled || currentAttempt !== attempt) {
        return;
      }
      if (!opened && !isReconnect) {
        setStatus("error");
        setMessage("websocket error");
      }
    });

    socket.addEventListener("message", (event) => {
      if (cancelled || currentAttempt !== attempt) {
        return;
      }
      const data =
        event.data instanceof ArrayBuffer
          ? new Uint8Array(event.data)
          : new TextEncoder().encode(String(event.data));
      const frame = decodeServerFrame(data);
      if (frame.type === "output") {
        terminalRef.current?.write(frame.data);
      } else {
        setStatus("error");
        setMessage(frame.message);
      }
    });
  }

  connect(false).catch((error: unknown) => {
    setStatus("error");
    setMessage(error instanceof Error ? error.message : "connection error");
  });

  return () => {
    cancelled = true;
    attempt += 1;
    clearReconnectTimer();
    socketRef.current?.close();
    socketRef.current = null;
  };
}, []);
```

- [ ] **Step 3: Run focused frontend tests**

Run:

```bash
npm --prefix frontend run test -- --run src/App.test.tsx
```

Expected: all `App` tests pass.

## Task 3: Rust Embed Asset Tests

**Files:**

- Modify: `/home/indexyz/terminald/crates/terminald-server/src/routes.rs`
- Modify: `/home/indexyz/terminald/crates/terminald-server/src/assets.rs`
- Modify: `/home/indexyz/terminald/.gitignore`
- Modify: `/home/indexyz/terminald/crates/terminald-server/assets/index.html`
- Delete: `/home/indexyz/terminald/crates/terminald-server/assets/.gitkeep`
- Delete: `/home/indexyz/terminald/crates/terminald-server/assets/assets/terminald.css`
- Delete: `/home/indexyz/terminald/crates/terminald-server/assets/assets/terminald.js`

**Ownership:** Server tests and embedded fallback files only. Do not implement `rust-embed` yet.

- [ ] **Step 1: Track only fallback index under embedded assets**

Change `.gitignore` asset rules to:

```gitignore
frontend/dist/
crates/terminald-server/assets/*
!crates/terminald-server/assets/index.html
```

Replace `/home/indexyz/terminald/crates/terminald-server/assets/index.html` with:

```html
<!doctype html>
<html lang="en">
  <head>
    <meta charset="UTF-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Terminald</title>
  </head>
  <body>
    <main>
      <h1>No frontend built</h1>
      <p>
        Run npm --prefix frontend run build to generate the Terminald web UI.
      </p>
    </main>
  </body>
</html>
```

Delete obsolete generated files outside `assets/dist`:

```bash
rm -f crates/terminald-server/assets/.gitkeep
rm -f crates/terminald-server/assets/assets/terminald.css crates/terminald-server/assets/assets/terminald.js
rmdir crates/terminald-server/assets/assets 2>/dev/null || true
```

- [ ] **Step 2: Update embedded route tests to assert fallback HTML**

In `serves_embedded_index_for_app_routes`, replace body assertions with:

```rust
assert!(body.contains("No frontend built"));
assert!(!body.contains(r#"src="./assets/terminald.js""#));
```

- [ ] **Step 3: Update embedded CSS test to match default fallback bundle**

Replace `serves_embedded_static_assets_under_prefixes` with:

```rust
#[tokio::test]
async fn missing_embedded_static_assets_return_not_found_under_prefixes() {
    let router = app_with_auth(AuthConfig::disabled());
    for path in ["/assets/terminald.css", "/aaa/assets/terminald.css"] {
        let response = get(router.clone(), path).await;
        assert_eq!(response.status(), StatusCode::NOT_FOUND);
    }
}
```

- [ ] **Step 4: Add embedded lookup selection helper and unit test**

Add these pure helpers near `embedded_asset` in `assets.rs`; they do not require `rust-embed` yet:

```rust
fn embedded_lookup_candidates(path: &str) -> [String; 2] {
    [format!("dist/{path}"), path.to_string()]
}

fn select_embedded_candidate(
    path: &str,
    exists: impl Fn(&str) -> bool,
) -> Option<String> {
    embedded_lookup_candidates(path)
        .into_iter()
        .find(|candidate| exists(candidate))
}
```

Add this unit test in the new `assets.rs` test module:

```rust
#[test]
fn selects_generated_dist_asset_before_fallback_asset() {
    let selected = select_embedded_candidate("index.html", |candidate| {
        matches!(candidate, "dist/index.html" | "index.html")
    })
    .unwrap();
    assert_eq!(selected, "dist/index.html");
}
```

- [ ] **Step 5: Update protected HTTP route expectations for missing embedded assets**

In `protects_http_routes_when_auth_enabled`, keep unauthenticated requests expecting `401`, but assert authenticated app routes return `200` and authenticated missing static assets return `404`:

```rust
for path in ["/", "/aaa/", "/foo/route/", "/example/bbb/client/path/"] {
    assert_eq!(
        get(router.clone(), path).await.status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(get_with_auth(router.clone(), path).await.status(), StatusCode::OK);
}

let css = "/assets/terminald.css";
assert_eq!(get(router.clone(), css).await.status(), StatusCode::UNAUTHORIZED);
assert_eq!(get_with_auth(router.clone(), css).await.status(), StatusCode::NOT_FOUND);
```

- [ ] **Step 6: Run focused server asset tests and confirm failure**

Add asset-loader traversal unit tests in `assets.rs`; do not add traversal tests to `routes.rs`:

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn rejects_traversal_before_external_or_embedded_lookup() {
        let config = AssetConfig::embedded();
        for path in ["/..", "/../index.html", "/assets/..", "/assets/../index.html", "/assets/../secret.txt"] {
            assert!(config.load(path).await.unwrap().is_none());
        }

        let parent = tempfile::tempdir().unwrap();
        let dir = parent.path().join("dist");
        tokio::fs::create_dir_all(&dir).await.unwrap();
        tokio::fs::write(dir.join("index.html"), "external index")
            .await
            .unwrap();
        tokio::fs::write(dir.join("secret.txt"), "external secret")
            .await
            .unwrap();
        tokio::fs::write(parent.path().join("outside.txt"), "outside secret")
            .await
            .unwrap();
        let config = AssetConfig::with_external_dist(dir);
        for path in [
            "/..",
            "/../index.html",
            "/../outside.txt",
            "/assets/..",
            "/assets/../index.html",
            "/assets/../secret.txt",
            "/assets/../outside.txt",
        ] {
            assert!(config.load(path).await.unwrap().is_none());
        }
    }

}
```

Run:

```bash
cargo test -p terminald-server
```

Expected before implementation: build or tests fail because `assets.rs` still references deleted `assets/terminald.css` and `assets/terminald.js` through `include_bytes!`.

## Task 4: Rust Embed Asset Implementation

**Files:**

- Modify: `/home/indexyz/terminald/frontend/vite.config.ts`
- Modify: `/home/indexyz/terminald/Cargo.toml`
- Modify: `/home/indexyz/terminald/crates/terminald-server/Cargo.toml`
- Modify: `/home/indexyz/terminald/crates/terminald-server/src/assets.rs`

**Ownership:** Rust asset loader, dependency manifests, and Vite output path only.

- [ ] **Step 1: Move generated frontend output under embedded dist**

In `/home/indexyz/terminald/frontend/vite.config.ts`, change:

```ts
outDir: "../crates/terminald-server/assets",
```

to:

```ts
outDir: "../crates/terminald-server/assets/dist",
```

- [ ] **Step 2: Add rust-embed dependency**

Add to workspace dependencies in `/home/indexyz/terminald/Cargo.toml`:

```toml
rust-embed = "8.11.0"
```

Add to `/home/indexyz/terminald/crates/terminald-server/Cargo.toml` dependencies:

```toml
rust-embed.workspace = true
```

- [ ] **Step 3: Derive an embedded asset directory**

In `assets.rs`, add:

```rust
use rust_embed::RustEmbed;
```

Define:

```rust
#[derive(RustEmbed)]
#[folder = "assets"]
struct EmbeddedAssets;
```

- [ ] **Step 4: Reject traversal before route normalization, external lookup, or embedded lookup**

In `AssetConfig::load`, insert the traversal guard as the first statement, before `asset_relative_path(request_path)`:

```rust
if has_parent_segment(request_path) {
    return Ok(None);
}
```

Add this helper near `path_has_extension`:

```rust
fn has_parent_segment(path: &str) -> bool {
    path.split('/').any(|segment| segment == "..")
}
```

- [ ] **Step 5: Replace manual embedded asset match**

Replace `embedded_asset` with:

```rust
fn embedded_asset(path: &str) -> Result<Option<Asset>> {
    let Some(candidate) = select_embedded_candidate(path, |candidate| {
        EmbeddedAssets::get(candidate).is_some()
    }) else {
        return Ok(None);
    };
    let Some(file) = EmbeddedAssets::get(&candidate) else {
        return Ok(None);
    };
    Ok(Some(Asset::new(file.data.into_owned(), content_type(path))))
}
```

- [ ] **Step 6: Run focused server asset tests**

Run:

```bash
cargo test -p terminald-server
```

Expected: tests pass.

## Task 5: Documentation And Full Verification

**Files:**

- Modify: `/home/indexyz/terminald/README.md`

**Ownership:** Docs and verification only.

- [ ] **Step 1: Update README build section**

Change the build notes to explain:

```md
`cargo build --workspace` works without a frontend build; the server embeds a checked-in fallback page that says `No frontend built`.

Run `npm --prefix frontend run build` before release packaging or local full-UI testing. The frontend build writes generated assets into `crates/terminald-server/assets/dist`, where `rust-embed` includes them at compile time. Generated JS and CSS remain ignored source artifacts; the checked-in fallback `index.html` keeps Rust-only builds working and is not overwritten by the frontend build.
```

- [ ] **Step 2: Update README frontend/reconnect behavior note**

Add to Reverse Proxy Paths or Server section:

```md
The browser reconnects automatically after an established WebSocket disconnects. A reconnect starts a fresh PTY session; Terminald does not replay prior terminal output or resume the old process.
```

- [ ] **Step 3: Run formatting and targeted tests**

Run:

```bash
cargo fmt
npm --prefix frontend run test -- --run src/App.test.tsx
cargo test -p terminald-server
```

Expected: commands pass.

- [ ] **Step 4: Run full verification**

Run:

```bash
cargo fmt --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
npm --prefix frontend run lint
npm --prefix frontend run test -- --run
fallback_hash_before="$(sha256sum crates/terminald-server/assets/index.html | cut -d ' ' -f1)"
npm --prefix frontend run build
fallback_hash_after="$(sha256sum crates/terminald-server/assets/index.html | cut -d ' ' -f1)"
test -f crates/terminald-server/assets/dist/index.html
rg -n "No frontend built" crates/terminald-server/assets/index.html
test "$fallback_hash_before" = "$fallback_hash_after"
test ! -e crates/terminald-server/assets/.gitkeep
```

Expected: all commands pass.

- [ ] **Step 5: Inspect final diff**

Run:

```bash
git status --short
git diff --stat
git diff -- . ':!frontend/package-lock.json' ':!docs/superpowers/specs/**' ':!docs/superpowers/plans/**'
```

Expected: only intended frontend, Rust asset, README, and dependency lockfile changes are present outside the SDD spec/plan artifacts.
