# Diagnostic Terminal Shell Design

## Goal

Upgrade Terminald's current minimal browser terminal shell into a restrained, diagnostic operations surface for small internal engineering teams. The terminal content remains the primary interface. The surrounding UI should clarify authentication, WebSocket, reconnect, remote-exit, and network state without adding navigation, side panels, or first-phase controls.

## Product Context

Terminald is a tool UI for small team operations and debugging. Users need a trusted browser terminal that is quick to enter, clear when state changes, and useful when something fails. The interface should feel reliable, restrained, secure, efficient, modern, and diagnostic.

The design follows the root `PRODUCT.md` context:

- Keep terminal content primary.
- Make state legible.
- Prefer operational trust over novelty.
- Reduce uncertainty at failure boundaries.
- Preserve speed of use.

## Scope

This first phase covers the frontend terminal shell only:

- Replace ad hoc `status`/`message` strings with a structured `ConnectionState` model.
- Add a compact top `ConnectionStatusBar` component.
- Improve status copy for connection, reconnect, authentication, WebSocket, remote-exit, and initial network failures.
- Preserve terminal output after remote command exit and show the exit code in the status bar.
- Show a short reconnect-success notice that clarifies a new PTY session was created.
- Preserve the existing path-prefix behavior for `auth/check` and `ws` URL resolution.
- Add focused frontend tests for state mapping, lifecycle behavior, notice timing, and CSS contract.

This phase intentionally excludes:

- Multi-session UI.
- Session resume or terminal output replay.
- Server command display.
- WebSocket protocol changes.
- CLI changes.
- New server endpoints.
- Status bar action buttons.
- Copy-diagnostics controls.
- Tooltips or diagnostic drawers.
- Full i18n infrastructure.
- Marketing pages or visual redesigns outside the terminal shell.

## Interface Structure

Use a compact top status bar as the only terminal-adjacent chrome. The page remains a two-row layout:

1. `ConnectionStatusBar` with stable single-line height.
2. `GhosttyTerminal` filling the remaining viewport.

The terminal must not be placed in a decorative card. No sidebar, drawer, or bottom toolbar is part of this phase. The status bar may use a status dot, status-specific text color, and subtle background or border changes. It must not use a full-width high-saturation warning treatment for ordinary reconnect or closed states.

On narrow viewports, the status bar stays single-line and truncates overflow. The primary status phrase must remain visible before any lower-priority detail.

## Components

### `App`

`App` keeps ownership of the connection lifecycle:

- `auth/check` requests.
- WebSocket creation and event handling.
- Reconnect timer management.
- Latest terminal size storage and resize resend on open.
- Remote `Exited` frame handling.
- Passing the current `ConnectionState` to `ConnectionStatusBar`.

The existing reconnect semantics remain: after an established WebSocket disconnects, the frontend retries and a successful reconnect creates a fresh PTY session. The frontend does not claim to resume the prior remote process.

### `ConnectionStatusBar`

`ConnectionStatusBar` renders the compact top status line. It should be a display-only component in this phase. It must not own connection side effects or expose buttons.

It should receive a `ConnectionState` and map it to:

- A short primary label.
- An optional short detail.
- A semantic visual tone.
- Accessible text for the live region.

Suggested examples:

- `Connected · ws ready`
- `Connected · new PTY session`
- `Reconnecting · retrying connection`
- `Auth required · credentials rejected`
- `Error · auth check failed`
- `Error · websocket upgrade failed`
- `Error · network down`
- `Error · server reported: invalid resize payload`
- `Closed · remote exited 0`

The copy should default to English. Avoid scattering text across unrelated code paths so future i18n remains possible, but do not add an i18n framework in this phase.

The status view-model should map `ConnectionState` into a primary label plus optional detail in a single place. The rendered text order is always `primary · detail` when detail exists. The primary label must appear before detail in DOM order so narrow-screen truncation preserves the most important state first.

### `GhosttyTerminal`

`GhosttyTerminal` keeps its current role:

- Initialize `ghostty-web`.
- Fit to container.
- Focus the terminal.
- Forward terminal input and resize events.
- Expose `write(data)` to `App`.

Terminal initialization/loading state is not a required status in this phase.

## State Model

Introduce a frontend-internal `ConnectionState` model. The precise TypeScript shape may be adjusted during implementation, but it should preserve these concepts:

```ts
type ConnectionPhase =
  | "connecting"
  | "connected"
  | "reconnecting"
  | "closed"
  | "error";

type ConnectionReason =
  | "auth_required"
  | "auth_check_failed"
  | "network"
  | "websocket_upgrade"
  | "websocket_error"
  | "server_error"
  | "remote_exit";

type ConnectionNotice = "new_session";

type ConnectionState = {
  phase: ConnectionPhase;
  reason?: ConnectionReason;
  detail?: string;
  exitCode?: number;
  notice?: ConnectionNotice;
};
```

`phase` drives the broad lifecycle. `reason` identifies the boundary that produced the state. `detail` carries a short user-visible diagnostic summary when useful. `exitCode` is used for remote process exit. `notice` is a transient presentation detail and must not change the underlying connection phase.

## Data Flow

Initial load:

1. Set `phase: "connecting"`.
2. Run `auth/check`.
3. On `401`, set `phase: "error", reason: "auth_required"`.
4. On non-`204`, set `phase: "error", reason: "auth_check_failed"`.
5. On fetch rejection, set `phase: "error", reason: "network", detail`.
6. On auth success, open WebSocket.
7. On WebSocket open, set `phase: "connected"` and send latest resize if present.
8. On initial WebSocket `error` before open, set `phase: "error", reason: "websocket_error"`.
9. On immediate close before open, set `phase: "error", reason: "websocket_upgrade"`. If the browser reports both an initial `error` and then `close`, the later close-before-open state wins because it is the final failed-upgrade outcome users can act on.

Established disconnect:

1. On close after open, set `phase: "reconnecting"`.
2. Retry after the existing fixed reconnect delay.
3. During reconnect, repeat `auth/check` before opening the next socket.
4. On reconnect-time fetch rejection, remain `phase: "reconnecting", reason: "network", detail` and retry again.
5. On reconnect-time `401`, stop and set `phase: "error", reason: "auth_required"`.
6. On reconnect-time non-`204`, stop and set `phase: "error", reason: "auth_check_failed"`.
7. On reconnect WebSocket close before open, remain reconnecting and retry.
8. On reconnect WebSocket open, set `phase: "connected", notice: "new_session"` for about 2 seconds, then clear `notice` back to ordinary connected display.

On reconnect WebSocket `error` before open, keep showing reconnecting status and allow the following close or retry timer path to own the retry behavior. Do not stop reconnecting just because a reconnect-attempt socket emitted `error`.

Any state change away from connected must cancel and clear a pending `new_session` notice so stale reconnect-success copy cannot remain visible during a later reconnect, error, or closed state.

Remote exit:

1. On `ServerMessage::Exited(code)`, set `phase: "closed", reason: "remote_exit", exitCode: code`.
2. Close the socket.
3. Do not schedule reconnect.
4. Preserve terminal output so the user can inspect final command output.

Server error frame:

1. On `ServerMessage::Error(message)`, set `phase: "error", reason: "server_error", detail: message`.
2. Do not close the socket solely because an error frame arrived.
3. Do not schedule reconnect solely because an error frame arrived.
4. Keep terminal output visible and continue processing later socket events according to the normal lifecycle.

Unmount cleanup must still close the active socket and cancel pending timers, including any transient notice timer.

## Error Handling

The status bar should retain a short lower-level summary where it helps diagnosis, such as a fetch error message. It must not display credentials, Basic auth secrets, or raw `Authorization` header values.

Error mapping should keep boundaries distinct:

- Authentication required: credentials rejected or missing.
- Auth check failed: server returned a non-`204` and non-`401` status.
- Network: fetch rejection or equivalent browser network failure.
- WebSocket upgrade: initial socket closes before open.
- WebSocket error: initial socket error before open. If a close-before-open follows, the upgrade-failure display wins.
- Server error: server sent a protocol error frame while the socket remains open.
- Remote exit: server reported a process exit code.

The UI should not show full stack traces or long error chains in the status bar. Long details should be normalized to a concise single-line message.

## Accessibility

The status bar should remain a non-interruptive live region. Use `role="status"` and an appropriate polite live behavior so dynamic updates can be announced without interrupting terminal input.

Visual requirements:

- Status text must meet WCAG AA contrast against its background.
- Error, reconnecting, closed, and connected tones must not rely on color alone; the text itself must identify the state.
- Status bar height must be stable across states.
- Reduced motion preferences must be respected. This phase does not require animated transitions.

## Styling Direction

Use the existing restrained terminal palette as a starting point rather than introducing a new theme. The first phase may refine status bar tokens, but it should avoid a one-note decorative palette and avoid saturated warning surfaces that compete with terminal content.

Recommended visual vocabulary:

- Dark operational surface.
- Subtle divider between status and terminal.
- Small status dot or indicator.
- Moderate tone differences for connected, reconnecting, error, and closed states.
- No gradients, decorative cards, or hero-like typography.

## Testing

Add focused frontend tests before or alongside implementation.

Required coverage:

- `ConnectionState` to status view-model mapping for connected, reconnecting, auth required, auth check failed, network error, WebSocket upgrade failure, and remote exit.
- Existing `App` lifecycle tests updated to assert the richer status copy.
- Reconnect success from an established socket close shows `Connected · new PTY session` for about 2 seconds, then returns to ordinary connected copy.
- URL resolution tests continue to prove `auth/check` and `ws` resolve relative to the current page path for path-prefix deployments.
- Remote exit preserves the closed state and does not schedule reconnect.
- Server error frames map to `phase: "error", reason: "server_error"`, display a concise detail, and do not close the socket or schedule reconnect by themselves.
- Initial WebSocket `error` before open is distinguishable from close-before-open, and close-before-open wins when both events occur.
- Reconnect-time fetch failure keeps retrying while showing reconnecting/network status.
- Unmount cancels pending reconnect and pending new-session notice timers.
- CSS contract covers stable status bar height, single-line truncation, primary-before-detail DOM order, and distinct state selectors or data attributes.

Verification commands for implementation:

```bash
npm --prefix frontend run test -- --run
npm --prefix frontend run lint
npm --prefix frontend run build
```

If implementation only touches frontend files, Rust workspace tests are not required for this specific phase, but a full release branch should still run the workspace checks documented in `README.md` and `AGENTS.md`.

## Acceptance Criteria

- Terminal content remains the primary viewport area and is not wrapped in a decorative card.
- The top status bar is display-only in this phase.
- The status bar distinguishes connecting, connected, reconnecting, authentication failure, auth-check failure, network failure, WebSocket upgrade failure, WebSocket error before open, and remote exit.
- Server error frames remain visible as diagnostic status without closing the socket or scheduling reconnect by themselves.
- `auth/check` and `ws` continue to resolve relative to the current page path for reverse-proxy path-prefix deployments.
- Reconnect success briefly communicates that a new PTY session was created.
- Remote exit keeps terminal output visible and shows the exit code in the status bar.
- No credentials or raw auth headers are displayed in status details.
- Status text and visual state treatment meet WCAG AA contrast expectations.
- Narrow viewports keep a stable single-line status bar with truncation.
- New and modified files under `frontend/src` remain below 400 LOC.
- Frontend tests, lint, and build pass after implementation.

## Follow-Up Directions

After this phase, the most valuable UX follow-ups are:

1. Manual reconnect and copy-diagnostics actions in the status bar.
2. Terminal operation efficiency improvements such as full-screen mode, safer paste behavior, and better focus recovery.
3. Session-level UX for future multi-session or shared/read-only access.
