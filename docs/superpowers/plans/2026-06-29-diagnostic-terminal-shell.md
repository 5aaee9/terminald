# Diagnostic Terminal Shell Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build a compact diagnostic terminal shell that represents connection lifecycle state with a structured model and a display-only top status bar.

**Architecture:** Add a small frontend state module that owns `ConnectionState`, display mapping, detail normalization, and the fixed reconnect-success notice duration. Add a display-only React status-bar component and styles that render the view-model with stable single-line layout and semantic data attributes. Refactor `App` to set structured connection state for auth, WebSocket, reconnect, server error frames, and remote exit without changing URL resolution, protocol, CLI, or backend behavior.

**Tech Stack:** React 19, TypeScript, Vite, Vitest, Testing Library, existing `ghostty-web` terminal adapter.

## Global Constraints

- Keep changes scoped to the frontend diagnostic terminal shell.
- Do not change the WebSocket protocol, CLI behavior, server endpoints, auth semantics, or backend behavior.
- Preserve existing path-prefix behavior: `auth/check` and `ws` must continue resolving from the current page path through `frontend/src/terminal/urls.ts`.
- The top status bar is display-only in this phase; do not add buttons, tooltips, drawers, copy-diagnostics actions, or manual reconnect controls.
- Do not wrap the terminal in a decorative card, add sidebars, or introduce marketing/hero UI.
- Default UI copy remains English; do not add an i18n framework.
- Do not display credentials, Basic auth secrets, or raw `Authorization` header values in status details.
- Use `NEW_SESSION_NOTICE_MS = 2000` for the reconnect-success notice duration.
- Status view-model precedence is `notice > detail > none`.
- Normalize long or multi-line status details in the state view-model layer, not at event producers.
- The status bar must render primary text, then a real accessible separator, then detail in DOM order and stay single-line with truncation on narrow viewports.
- Expose semantic styling hooks as `data-phase`, `data-reason`, and `data-notice` on the status bar.
- New and modified files under `frontend/src` must remain below 400 LOC.
- Preserve lower-level error information as concise frontend details where useful, without leaking credentials.
- Required frontend verification after implementation: `npm --prefix frontend run test -- --run`, `npm --prefix frontend run lint`, and `npm --prefix frontend run build`.
- Because Markdown docs are touched, run `nix fmt` before final completion.

---

## File Structure

- Create `frontend/src/terminal/connectionState.ts`: connection state types, `NEW_SESSION_NOTICE_MS`, detail normalization, and `connectionStatusView(state)`.
- Create `frontend/src/terminal/connectionState.test.ts`: focused unit tests for view-model mapping, normalization, precedence, and credential redaction expectations.
- Create `frontend/src/terminal/ConnectionStatusBar.tsx`: display-only status bar component that renders primary/detail text and semantic data attributes.
- Create `frontend/src/terminal/ConnectionStatusBar.test.tsx`: component rendering and accessibility tests.
- Modify `frontend/src/App.tsx`: replace string status/message state with `ConnectionState`, manage reconnect notice timer, map lifecycle events to structured state, and pass state to `ConnectionStatusBar`.
- Modify `frontend/src/App.test.tsx`: update existing lifecycle assertions to the richer copy.
- Create `frontend/src/App.connection.test.tsx`: add new server-error, notice, URL preservation, WebSocket precedence, and timer cleanup coverage without pushing `App.test.tsx` over 400 LOC.
- Modify `frontend/src/styles.css`: replace/extend `.status` styling for the status bar with stable single-line layout and semantic state tones.
- Modify `frontend/src/styles.test.ts`: add CSS contract checks for stable status height, truncation, primary/detail selectors, and semantic data attributes.
- Keep `frontend/src/terminal/urls.ts` unchanged unless tests reveal a regression; path-prefix URL behavior should remain covered by existing tests and App lifecycle tests.
- Keep `frontend/src/terminal/GhosttyTerminal.tsx` unchanged.

---

### Task 1: Add Connection State View Model

**Files:**

- Create: `frontend/src/terminal/connectionState.ts`
- Create: `frontend/src/terminal/connectionState.test.ts`

**Interfaces:**

- Produces: `NEW_SESSION_NOTICE_MS: 2000`
- Produces: `ConnectionPhase`, `ConnectionReason`, `ConnectionNotice`, `ConnectionState`, `ConnectionStatusTone`, `ConnectionStatusView`
- Produces: `connectionStatusView(state: ConnectionState): ConnectionStatusView`
- Produces: `normalizeStatusDetail(detail: string): string`
- Consumed by: Task 2 `ConnectionStatusBar`; Task 3 `App`

- [ ] **Step 1: Write failing view-model tests**

Create `frontend/src/terminal/connectionState.test.ts` with these tests:

```ts
import { describe, expect, it } from "vitest";

import {
  NEW_SESSION_NOTICE_MS,
  connectionStatusView,
  normalizeStatusDetail,
  type ConnectionState,
} from "./connectionState";

describe("connectionStatusView", () => {
  it("exports the fixed new-session notice duration", () => {
    expect(NEW_SESSION_NOTICE_MS).toBe(2000);
  });

  it.each([
    [{ phase: "connecting" }, "Connecting", "auth check pending", "connecting"],
    [{ phase: "connected" }, "Connected", "ws ready", "connected"],
    [
      { phase: "reconnecting" },
      "Reconnecting",
      "retrying connection",
      "reconnecting",
    ],
    [
      { phase: "error", reason: "auth_required" },
      "Auth required",
      "credentials rejected",
      "error",
    ],
    [
      { phase: "error", reason: "auth_check_failed" },
      "Error",
      "auth check failed",
      "error",
    ],
    [
      { phase: "error", reason: "network", detail: "network down" },
      "Error",
      "network down",
      "error",
    ],
    [
      { phase: "error", reason: "websocket_upgrade" },
      "Error",
      "websocket upgrade failed",
      "error",
    ],
    [
      { phase: "error", reason: "websocket_error" },
      "Error",
      "websocket error",
      "error",
    ],
    [
      {
        phase: "error",
        reason: "server_error",
        detail: "invalid resize payload",
      },
      "Error",
      "server reported: invalid resize payload",
      "error",
    ],
    [
      { phase: "closed", reason: "remote_exit", exitCode: 0 },
      "Closed",
      "remote exited 0",
      "closed",
    ],
  ] satisfies Array<[ConnectionState, string, string, string]>)(
    "maps %#",
    (state, primary, detail, tone) => {
      expect(connectionStatusView(state)).toMatchObject({
        primary,
        detail,
        tone,
      });
    },
  );

  it("lets the new-session notice override the ordinary connected detail", () => {
    expect(
      connectionStatusView({ phase: "connected", notice: "new_session" }),
    ).toMatchObject({
      primary: "Connected",
      detail: "new PTY session",
      tone: "connected",
    });
  });

  it("falls back to closed idle copy when no closed reason applies", () => {
    expect(connectionStatusView({ phase: "closed" })).toMatchObject({
      primary: "Closed",
      detail: "connection closed",
      tone: "closed",
    });
  });

  it("normalizes details to concise single-line text", () => {
    expect(normalizeStatusDetail(" first line\nsecond line\tthird line ")).toBe(
      "first line second line third line",
    );
    expect(normalizeStatusDetail("x".repeat(160))).toHaveLength(120);
  });

  it("redacts credential-like details", () => {
    expect(normalizeStatusDetail("Authorization: Basic abc123")).toBe(
      "[redacted]",
    );
    expect(normalizeStatusDetail("Basic abc123")).toBe("[redacted]");
    expect(normalizeStatusDetail("credential user:secret rejected")).toBe(
      "[redacted]",
    );
    expect(normalizeStatusDetail("credentials user:secret rejected")).toBe(
      "[redacted]",
    );
  });
});
```

- [ ] **Step 2: Run the focused test and verify it fails**

Run: `npm --prefix frontend run test -- --run src/terminal/connectionState.test.ts`

Expected: FAIL because `frontend/src/terminal/connectionState.ts` does not exist.

- [ ] **Step 3: Implement the connection state module**

Create `frontend/src/terminal/connectionState.ts` with this implementation:

```ts
export const NEW_SESSION_NOTICE_MS = 2000;
const MAX_DETAIL_LENGTH = 120;

export type ConnectionPhase =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed"
  | "error";

export type ConnectionReason =
  | "auth_required"
  | "auth_check_failed"
  | "network"
  | "websocket_upgrade"
  | "websocket_error"
  | "server_error"
  | "remote_exit";

export type ConnectionNotice = "new_session";

export interface ConnectionState {
  phase: ConnectionPhase;
  reason?: ConnectionReason;
  detail?: string;
  exitCode?: number;
  notice?: ConnectionNotice;
}

export type ConnectionStatusTone =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed"
  | "error";

export interface ConnectionStatusView {
  primary: string;
  detail?: string;
  tone: ConnectionStatusTone;
}

export function normalizeStatusDetail(detail: string): string {
  const normalized = detail.replace(/\s+/g, " ").trim();
  if (
    /authorization\s*:/i.test(normalized) ||
    /\bbasic\s+\S+/i.test(normalized) ||
    /\bcredentials?\b/i.test(normalized)
  ) {
    return "[redacted]";
  }
  return normalized.length > MAX_DETAIL_LENGTH
    ? normalized.slice(0, MAX_DETAIL_LENGTH)
    : normalized;
}

export function connectionStatusView(
  state: ConnectionState,
): ConnectionStatusView {
  if (state.phase === "connected" && state.notice === "new_session") {
    return {
      primary: "Connected",
      detail: "new PTY session",
      tone: "connected",
    };
  }

  switch (state.phase) {
    case "connecting":
      return {
        primary: "Connecting",
        detail: "auth check pending",
        tone: "connecting",
      };
    case "connected":
      return {
        primary: "Connected",
        detail: state.detail ? normalizeStatusDetail(state.detail) : "ws ready",
        tone: "connected",
      };
    case "reconnecting":
      return {
        primary: "Reconnecting",
        detail: state.detail
          ? normalizeStatusDetail(state.detail)
          : "retrying connection",
        tone: "reconnecting",
      };
    case "closed":
      if (
        state.reason === "remote_exit" &&
        typeof state.exitCode === "number"
      ) {
        return {
          primary: "Closed",
          detail: `remote exited ${state.exitCode}`,
          tone: "closed",
        };
      }
      return {
        primary: "Closed",
        detail: state.detail
          ? normalizeStatusDetail(state.detail)
          : "connection closed",
        tone: "closed",
      };
    case "error":
      return errorStatusView(state);
  }
}

function errorStatusView(state: ConnectionState): ConnectionStatusView {
  if (state.reason === "auth_required") {
    return {
      primary: "Auth required",
      detail: "credentials rejected",
      tone: "error",
    };
  }
  if (state.reason === "auth_check_failed") {
    return { primary: "Error", detail: "auth check failed", tone: "error" };
  }
  if (state.reason === "network") {
    return {
      primary: "Error",
      detail: state.detail
        ? normalizeStatusDetail(state.detail)
        : "network failure",
      tone: "error",
    };
  }
  if (state.reason === "websocket_upgrade") {
    return {
      primary: "Error",
      detail: "websocket upgrade failed",
      tone: "error",
    };
  }
  if (state.reason === "websocket_error") {
    return {
      primary: "Error",
      detail: state.detail
        ? normalizeStatusDetail(state.detail)
        : "websocket error",
      tone: "error",
    };
  }
  if (state.reason === "server_error") {
    const detail = state.detail
      ? normalizeStatusDetail(state.detail)
      : "unknown error";
    return {
      primary: "Error",
      detail: `server reported: ${detail}`,
      tone: "error",
    };
  }
  return {
    primary: "Error",
    detail: state.detail
      ? normalizeStatusDetail(state.detail)
      : "connection error",
    tone: "error",
  };
}
```

- [ ] **Step 4: Run the focused test and verify it passes**

Run: `npm --prefix frontend run test -- --run src/terminal/connectionState.test.ts`

Expected: PASS.

- [ ] **Step 5: Check frontend source line counts**

Run: `find frontend/src -name '*.*' -print0 | xargs -0 wc -l | sort -n`

Expected: every file in `frontend/src` is below 400 LOC.

---

### Task 2: Add Display-Only Connection Status Bar

**Files:**

- Create: `frontend/src/terminal/ConnectionStatusBar.tsx`
- Create: `frontend/src/terminal/ConnectionStatusBar.test.tsx`
- Modify: `frontend/src/styles.css`
- Modify: `frontend/src/styles.test.ts`

**Interfaces:**

- Consumes: `ConnectionState` and `connectionStatusView(state)` from `frontend/src/terminal/connectionState.ts`
- Produces: `ConnectionStatusBar({ state }: { state: ConnectionState })`
- Produces: `.status`, `.status__indicator`, `.status__primary`, `.status__detail` styling and `data-phase`, `data-reason`, `data-notice` attributes consumed by CSS and tests

- [ ] **Step 1: Write failing component tests**

Create `frontend/src/terminal/ConnectionStatusBar.test.tsx` with these tests:

```tsx
import { render, screen } from "@testing-library/react";
import { describe, expect, it } from "vitest";

import ConnectionStatusBar from "./ConnectionStatusBar";

describe("ConnectionStatusBar", () => {
  it("renders primary text before detail in a polite status region", () => {
    render(<ConnectionStatusBar state={{ phase: "connected" }} />);

    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveAttribute("data-phase", "connected");
    expect(status).not.toHaveAttribute("data-reason");
    expect(status).toHaveTextContent("Connected · ws ready");

    const primary = status.querySelector(".status__primary");
    const separator = status.querySelector(".status__separator");
    const detail = status.querySelector(".status__detail");
    expect(primary).toHaveTextContent("Connected");
    expect(separator).toHaveTextContent("·");
    expect(detail).toHaveTextContent("ws ready");
    expect(primary?.compareDocumentPosition(separator as Node)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
    expect(separator?.compareDocumentPosition(detail as Node)).toBe(
      Node.DOCUMENT_POSITION_FOLLOWING,
    );
  });

  it("sets semantic reason and notice attributes", () => {
    render(
      <ConnectionStatusBar
        state={{ phase: "connected", notice: "new_session" }}
      />,
    );

    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("data-phase", "connected");
    expect(status).toHaveAttribute("data-notice", "new_session");
    expect(status).toHaveTextContent("Connected · new PTY session");
  });

  it("sets semantic reason attributes for error states", () => {
    render(
      <ConnectionStatusBar
        state={{ phase: "error", reason: "server_error", detail: "bad resize" }}
      />,
    );

    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("data-phase", "error");
    expect(status).toHaveAttribute("data-reason", "server_error");
    expect(status).toHaveTextContent("Error · server reported: bad resize");
  });
});
```

- [ ] **Step 2: Add failing CSS contract tests**

Modify `frontend/src/styles.test.ts` by adding these tests inside the existing `describe("terminal styles", () => { ... })` block:

```ts
it("keeps the status bar single-line with stable height", () => {
  expect(styles).toMatch(/\.status\s*\{[^}]*min-height:\s*24px;/s);
  expect(styles).toMatch(/\.status\s*\{[^}]*white-space:\s*nowrap;/s);
  expect(styles).toMatch(/\.status\s*\{[^}]*overflow:\s*hidden;/s);
});

it("styles primary and detail status text separately", () => {
  expect(styles).toContain(".status__primary");
  expect(styles).toContain(".status__separator");
  expect(styles).toContain(".status__detail");
  expect(styles).toMatch(
    /\.status__detail\s*\{[^}]*text-overflow:\s*ellipsis;/s,
  );
});

it("uses semantic data attributes for status tones", () => {
  expect(styles).toContain('.status[data-phase="connected"]');
  expect(styles).toContain('.status[data-phase="reconnecting"]');
  expect(styles).toContain('.status[data-phase="error"]');
  expect(styles).toContain('.status[data-phase="closed"]');
});
```

- [ ] **Step 3: Run focused tests and verify they fail**

Run: `npm --prefix frontend run test -- --run src/terminal/ConnectionStatusBar.test.tsx src/styles.test.ts`

Expected: FAIL because `ConnectionStatusBar.tsx` does not exist and the new CSS contract is not implemented.

- [ ] **Step 4: Implement `ConnectionStatusBar`**

Create `frontend/src/terminal/ConnectionStatusBar.tsx` with this implementation:

```tsx
import { connectionStatusView, type ConnectionState } from "./connectionState";

interface Props {
  state: ConnectionState;
}

export default function ConnectionStatusBar({ state }: Props) {
  const view = connectionStatusView(state);

  return (
    <div
      className="status"
      role="status"
      aria-live="polite"
      data-phase={state.phase}
      data-reason={state.reason}
      data-notice={state.notice}
    >
      <span className="status__indicator" aria-hidden="true" />
      <span className="status__primary">{view.primary}</span>
      {view.detail ? <span className="status__separator">{" · "}</span> : null}
      {view.detail ? (
        <span className="status__detail">{view.detail}</span>
      ) : null}
    </div>
  );
}
```

- [ ] **Step 5: Update status bar styles**

In `frontend/src/styles.css`, replace the existing `.status` rule with this block:

```css
.status {
  align-items: center;
  background: #18212a;
  border-bottom: 1px solid #2b3945;
  color: #aab8c5;
  display: flex;
  font-size: 12px;
  gap: 0;
  line-height: 24px;
  min-height: 24px;
  overflow: hidden;
  padding: 0 8px;
  white-space: nowrap;
}

.status__indicator {
  background: #7d8b99;
  border-radius: 999px;
  flex: 0 0 auto;
  height: 7px;
  margin-right: 6px;
  width: 7px;
}

.status__primary {
  color: #d8e0e7;
  flex: 0 0 auto;
  font-weight: 600;
}

.status__detail {
  color: #aab8c5;
  min-width: 0;
  overflow: hidden;
  text-overflow: ellipsis;
}

.status__separator {
  color: #7d8b99;
  flex: 0 0 auto;
}

.status[data-phase="connected"] {
  background: #16211d;
  border-bottom-color: #294236;
}

.status[data-phase="connected"] .status__indicator {
  background: #67c58f;
}

.status[data-phase="reconnecting"],
.status[data-phase="connecting"] {
  background: #1d2028;
  border-bottom-color: #35405a;
}

.status[data-phase="reconnecting"] .status__indicator,
.status[data-phase="connecting"] .status__indicator {
  background: #8da8ff;
}

.status[data-phase="error"] {
  background: #271b1d;
  border-bottom-color: #573138;
}

.status[data-phase="error"] .status__indicator {
  background: #ff8b8b;
}

.status[data-phase="closed"] {
  background: #211f1b;
  border-bottom-color: #494235;
}

.status[data-phase="closed"] .status__indicator {
  background: #d4a85f;
}
```

- [ ] **Step 6: Run focused tests and verify they pass**

Run: `npm --prefix frontend run test -- --run src/terminal/ConnectionStatusBar.test.tsx src/styles.test.ts`

Expected: PASS.

- [ ] **Step 7: Check frontend source line counts**

Run: `find frontend/src -name '*.*' -print0 | xargs -0 wc -l | sort -n`

Expected: every file in `frontend/src` is below 400 LOC.

---

### Task 3: Integrate Structured State Into App Lifecycle

**Files:**

- Modify: `frontend/src/App.tsx`
- Modify: `frontend/src/App.test.tsx`
- Create: `frontend/src/App.connection.test.tsx`

**Interfaces:**

- Consumes: `ConnectionState`, `NEW_SESSION_NOTICE_MS` from `frontend/src/terminal/connectionState.ts`
- Consumes: `ConnectionStatusBar` from `frontend/src/terminal/ConnectionStatusBar.tsx`
- Preserves: `resolveAuthCheckUrl()` and `resolveWebSocketUrl()` path-prefix behavior
- Preserves: existing input, resize, reconnect, remote-exit, and unmount cleanup behavior

- [ ] **Step 1: Update existing status expectations to the richer copy**

Modify `frontend/src/App.test.tsx` expected status text as follows:

- `connecting` -> `Connecting · auth check pending`.
- `connected` -> `Connected · ws ready`.
- The existing reconnect-success assertion inside `reconnects after an established websocket closes` should expect `Connected · new PTY session` immediately after the reconnect socket opens.
- `reconnecting` -> `Reconnecting · retrying connection`, except reconnect-time fetch failure with detail should assert `Reconnecting · network down`.
- `authentication required` -> `Auth required · credentials rejected`.
- `authentication check failed` -> `Error · auth check failed`.
- Initial auth-check fetch failure with `new Error("network down")` -> `Error · network down`.
- `authentication or websocket upgrade failed` -> `Error · websocket upgrade failed`.
- `websocket error` -> `Error · websocket error`.
- `Remote exited with code -1` -> `Closed · remote exited -1`.
- The existing `stops reconnecting when the remote command exits` test must keep asserting that advancing the reconnect delay does not create a second socket after an exit frame and close event.

Keep the existing tests and update only their expected copy unless a new assertion is called out below.

- [ ] **Step 2: Add failing App lifecycle tests for new requirements**

Create `frontend/src/App.connection.test.tsx` with the same mock socket, `GhosttyTerminal` mock, `flushEffects`, and `advanceReconnectDelay` helpers used in `frontend/src/App.test.tsx`, then add these focused tests:

```tsx
import { act, cleanup, render, screen, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

import App from "./App";

const sockets: MockSocket[] = [];

class MockSocket extends EventTarget {
  static OPEN = 1;
  readyState = MockSocket.OPEN;
  binaryType = "blob";
  sent: unknown[] = [];

  constructor(public url: string) {
    super();
    sockets.push(this);
  }

  send(data: unknown) {
    this.sent.push(data);
  }

  close() {
    this.readyState = 3;
  }

  open() {
    this.dispatchEvent(new Event("open"));
  }

  fail() {
    this.dispatchEvent(new Event("error"));
  }

  closeEvent() {
    this.dispatchEvent(new Event("close"));
  }

  message(data: Uint8Array) {
    this.dispatchEvent(new MessageEvent("message", { data: data.buffer }));
  }
}

vi.mock("./terminal/GhosttyTerminal", () => ({
  default: ({
    onData,
    onResize,
  }: {
    onData(data: string): void;
    onResize(cols: number, rows: number): void;
  }) => (
    <button
      type="button"
      onClick={() => {
        onData("a");
        onResize(80, 24);
      }}
    >
      terminal
    </button>
  ),
}));

async function flushEffects() {
  await act(async () => {
    await Promise.resolve();
  });
}

async function advanceReconnectDelay() {
  await act(async () => {
    vi.advanceTimersByTime(1000);
    await Promise.resolve();
  });
}

beforeEach(() => {
  sockets.length = 0;
  vi.stubGlobal("WebSocket", MockSocket);
  vi.stubGlobal(
    "fetch",
    vi.fn(async () => new Response(null, { status: 204 })),
  );
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  vi.unstubAllGlobals();
});

describe("App connection status", () => {
  it("shows a short new-session notice after reconnect opens", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].closeEvent();

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    sockets[1].open();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Connected · new PTY session",
    );

    await act(async () => {
      vi.advanceTimersByTime(2000);
      await Promise.resolve();
    });

    expect(screen.getByRole("status")).toHaveTextContent(
      "Connected · ws ready",
    );
  });

  it("clears a pending new-session notice when the reconnected socket closes", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].closeEvent();

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    sockets[1].open();
    await flushEffects();
    sockets[1].closeEvent();
    await flushEffects();

    expect(screen.getByRole("status")).toHaveTextContent(
      "Reconnecting · retrying connection",
    );
  });

  it("does not update state from a pending new-session notice after unmount", async () => {
    vi.useFakeTimers();
    const consoleError = vi
      .spyOn(console, "error")
      .mockImplementation(() => undefined);
    const view = render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].closeEvent();

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    sockets[1].open();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent(
      "Connected · new PTY session",
    );

    view.unmount();
    await act(async () => {
      vi.advanceTimersByTime(2000);
      await Promise.resolve();
    });

    expect(consoleError).not.toHaveBeenCalled();
    consoleError.mockRestore();
  });

  it("shows server error frames without closing the socket or scheduling reconnect", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();

    sockets[0].message(
      new Uint8Array([3, ...new TextEncoder().encode("bad resize")]),
    );
    await flushEffects();

    expect(screen.getByRole("status")).toHaveTextContent(
      "Error · server reported: bad resize",
    );
    expect(sockets[0].readyState).toBe(MockSocket.OPEN);

    await advanceReconnectDelay();
    expect(sockets).toHaveLength(1);
  });

  it("lets close-before-open win after an initial websocket error", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));

    sockets[0].fail();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent(
        "Error · websocket error",
      );
    });

    sockets[0].closeEvent();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent(
        "Error · websocket upgrade failed",
      );
    });
  });

  it("keeps auth and websocket URLs resolved from the current page path", async () => {
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    const originalPath = `${window.location.pathname}${window.location.search}${window.location.hash}`;
    window.history.pushState({}, "", "/prefix/tool/");

    try {
      render(<App />);
      await waitFor(() => expect(sockets).toHaveLength(1));
      expect(fetchMock).toHaveBeenCalledWith(
        `${window.location.origin}/prefix/tool/auth/check`,
        { credentials: "same-origin" },
      );
      expect(sockets[0].url).toBe(
        `ws://${window.location.host}/prefix/tool/ws`,
      );
    } finally {
      window.history.pushState({}, "", originalPath);
    }
  });
});
```

- [ ] **Step 3: Run the App tests and verify they fail**

Run: `npm --prefix frontend run test -- --run src/App.test.tsx src/App.connection.test.tsx`

Expected: FAIL because `App.tsx` still uses string status/message state and does not render `ConnectionStatusBar`.

- [ ] **Step 4: Refactor `App.tsx` state imports and refs**

In `frontend/src/App.tsx`:

- Import `ConnectionStatusBar`.
- Import `NEW_SESSION_NOTICE_MS` and `type ConnectionState`.
- Remove the old `type Status = ...` definition.
- Replace the `status` and `message` state with `connectionState`.
- Add a notice timer ref.

The top of the file should contain these imports and refs:

```tsx
import { useCallback, useEffect, useRef, useState } from "react";

import ConnectionStatusBar from "./terminal/ConnectionStatusBar";
import GhosttyTerminal, {
  type TerminalHandle,
} from "./terminal/GhosttyTerminal";
import {
  NEW_SESSION_NOTICE_MS,
  type ConnectionState,
} from "./terminal/connectionState";
import {
  decodeServerFrame,
  encodeInput,
  encodeResize,
} from "./terminal/protocol";
import { resolveAuthCheckUrl, resolveWebSocketUrl } from "./terminal/urls";

const RECONNECT_DELAY_MS = 1000;
```

Inside `App`, replace the old state declarations with:

```tsx
const [connectionState, setConnectionState] = useState<ConnectionState>({
  phase: "connecting",
});
```

Inside `useEffect`, add:

```tsx
let noticeTimer: ReturnType<typeof setTimeout> | null = null;

const clearNoticeTimer = () => {
  if (noticeTimer) {
    clearTimeout(noticeTimer);
    noticeTimer = null;
  }
};

const setState = (state: ConnectionState) => {
  if (state.notice !== "new_session") {
    clearNoticeTimer();
  }
  setConnectionState(state);
};

const showConnected = (isReconnect: boolean) => {
  if (!isReconnect) {
    setState({ phase: "connected" });
    return;
  }
  clearNoticeTimer();
  setState({ phase: "connected", notice: "new_session" });
  noticeTimer = setTimeout(() => {
    noticeTimer = null;
    setConnectionState((current) => {
      if (current.phase === "connected" && current.notice === "new_session") {
        return { phase: "connected" };
      }
      return current;
    });
  }, NEW_SESSION_NOTICE_MS);
};
```

- [ ] **Step 5: Refactor lifecycle event state assignments**

In `frontend/src/App.tsx`, replace every old `setStatus(...)` and `setMessage(...)` pair with structured state:

- Start of `connect(isReconnect)`: `setState({ phase: isReconnect ? "reconnecting" : "connecting" });`
- Initial fetch rejection: `setState({ phase: "error", reason: "network", detail: error instanceof Error ? error.message : "connection error" });`
- Reconnect fetch rejection: `setState({ phase: "reconnecting", reason: "network", detail: error instanceof Error ? error.message : "connection error" });` then `scheduleReconnect()`.
- Auth `401`: `setState({ phase: "error", reason: "auth_required" });`
- Auth non-`204`: `setState({ phase: "error", reason: "auth_check_failed" });`
- Socket open: `showConnected(isReconnect);`
- Socket close after opened or reconnect close-before-open: `scheduleReconnect();`
- Initial socket close-before-open: `setState({ phase: "error", reason: "websocket_upgrade" });`
- Initial socket error before open: `setState({ phase: "error", reason: "websocket_error" });`
- Server error frame: `setState({ phase: "error", reason: "server_error", detail: frame.message });`
- Server exited frame: set `remoteExited = true` before closing the socket, then `setState({ phase: "closed", reason: "remote_exit", exitCode: frame.code });`. This guard must suppress the subsequent socket close from scheduling reconnect.
- Top-level `connect(false).catch(...)`: `setState({ phase: "error", reason: "network", detail: error instanceof Error ? error.message : "connection error" });`

In `scheduleReconnect`, use:

```tsx
clearNoticeTimer();
setConnectionState((current) => {
  if (current.phase === "reconnecting" && current.reason === "network") {
    return current;
  }
  return { phase: "reconnecting" };
});
```

This preserves `Reconnecting · network down` after a reconnect-time fetch rejection until the next retry attempt updates state.

Keep the existing websocket `error` handler's `!isReconnect` guard. Initial socket errors before open map to `websocket_error`; reconnect-attempt socket errors before open keep the UI in reconnecting and leave retry scheduling to the close/retry path.

In cleanup, call both `clearReconnectTimer()` and `clearNoticeTimer()`.

- [ ] **Step 6: Render the status bar component**

Replace the old status div in `App.tsx`:

```tsx
<div className="status" role="status">
  {message}
</div>
```

with:

```tsx
<ConnectionStatusBar state={connectionState} />
```

Change the main element data attribute from the old string status to the structured phase:

```tsx
    <main className="terminal-shell" data-status={connectionState.phase}>
```

- [ ] **Step 7: Run App tests and fix any TypeScript/test issues**

Run: `npm --prefix frontend run test -- --run src/App.test.tsx src/App.connection.test.tsx`

Expected: PASS after implementation.

- [ ] **Step 8: Run URL tests to verify path-prefix behavior remains intact**

Run: `npm --prefix frontend run test -- --run src/terminal/urls.test.ts src/App.test.tsx src/App.connection.test.tsx`

Expected: PASS.

- [ ] **Step 9: Check frontend source line counts**

Run: `find frontend/src -name '*.*' -print0 | xargs -0 wc -l | sort -n`

Expected: every file in `frontend/src` is below 400 LOC. `App.connection.test.tsx` exists so the new lifecycle coverage does not push `App.test.tsx` over the limit.

---

### Task 4: Final Frontend Verification And Documentation Cleanup

**Files:**

- Inspect: `PRODUCT.md`
- Inspect: `docs/superpowers/specs/2026-06-29-diagnostic-terminal-shell-design.md`
- Inspect: `docs/superpowers/plans/2026-06-29-diagnostic-terminal-shell.md`
- Inspect: modified frontend files from Tasks 1-3

**Interfaces:**

- Consumes: completed Tasks 1-3
- Produces: fresh verification evidence for final implementation review and commit readiness

- [ ] **Step 1: Run frontend tests**

Run: `npm --prefix frontend run test -- --run`

Expected: PASS.

- [ ] **Step 2: Run frontend lint**

Run: `npm --prefix frontend run lint`

Expected: PASS.

- [ ] **Step 3: Run frontend build**

Run: `npm --prefix frontend run build`

Expected: PASS.

- [ ] **Step 4: Run Markdown/Nix formatting**

Run: `nix fmt`

Expected: PASS. If it formats unrelated tracked files, inspect and revert unrelated formatting before completion.

- [ ] **Step 5: Verify source line counts**

Run: `find frontend/src crates -name '*.*' -print0 | xargs -0 wc -l | sort -n`

Expected: every source file under `frontend/src` and every Rust source file under `crates` remains below 400 LOC.

- [ ] **Step 6: Verify status color contrast evidence**

Run this local contrast check for the planned status text/background pairs:

```bash
node - <<'NODE'
function hexToRgb(hex) {
  const value = Number.parseInt(hex.slice(1), 16);
  return [(value >> 16) & 255, (value >> 8) & 255, value & 255].map((channel) => {
    const normalized = channel / 255;
    return normalized <= 0.03928 ? normalized / 12.92 : ((normalized + 0.055) / 1.055) ** 2.4;
  });
}
function luminance(hex) {
  return hexToRgb(hex).reduce((sum, value, index) => sum + [0.2126, 0.7152, 0.0722][index] * value, 0);
}
function contrast(foreground, background) {
  const [high, low] = [luminance(foreground), luminance(background)].sort((a, b) => b - a);
  return (high + 0.05) / (low + 0.05);
}
const pairs = [
  ["primary/default", "#d8e0e7", "#18212a"],
  ["detail/default", "#aab8c5", "#18212a"],
  ["primary/connected", "#d8e0e7", "#16211d"],
  ["detail/connected", "#aab8c5", "#16211d"],
  ["primary/reconnecting", "#d8e0e7", "#1d2028"],
  ["detail/reconnecting", "#aab8c5", "#1d2028"],
  ["primary/error", "#d8e0e7", "#271b1d"],
  ["detail/error", "#aab8c5", "#271b1d"],
  ["primary/closed", "#d8e0e7", "#211f1b"],
  ["detail/closed", "#aab8c5", "#211f1b"],
];
for (const [name, foreground, background] of pairs) {
  const ratio = contrast(foreground, background);
  console.log(`${name}: ${ratio.toFixed(2)}`);
  if (ratio < 4.5) process.exitCode = 1;
}
NODE
```

Expected: every printed ratio is at least `4.50`.

- [ ] **Step 7: Confirm generated frontend assets were not intentionally modified**

Run: `git status --short`

Expected: source/docs/config changes only. No tracked or staged generated asset under `crates/terminald-server/assets/dist` should appear.

- [ ] **Step 8: Review final diff**

Run: `git diff -- PRODUCT.md docs/superpowers/specs/2026-06-29-diagnostic-terminal-shell-design.md docs/superpowers/plans/2026-06-29-diagnostic-terminal-shell.md frontend/src/App.tsx frontend/src/App.test.tsx frontend/src/App.connection.test.tsx frontend/src/styles.css frontend/src/styles.test.ts frontend/src/terminal/connectionState.ts frontend/src/terminal/connectionState.test.ts frontend/src/terminal/ConnectionStatusBar.tsx frontend/src/terminal/ConnectionStatusBar.test.tsx`

Expected: diff is limited to the diagnostic terminal shell spec/plan/context, status model/component, App integration, tests, and styles.

- [ ] **Step 9: Prepare final review evidence**

Record the exact outputs for:

```text
npm --prefix frontend run test -- --run
npm --prefix frontend run lint
npm --prefix frontend run build
nix fmt
find frontend/src crates -name '*.*' -print0 | xargs -0 wc -l | sort -n
status color contrast node check
git status --short
```

Expected: all verification commands succeed, line counts satisfy the 400 LOC rule, and working tree changes are intentional.
