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
    [{ phase: "reconnecting" }, "Reconnecting", "retrying connection", "reconnecting"],
    [{ phase: "error", reason: "auth_required" }, "Auth required", "credentials rejected", "error"],
    [{ phase: "error", reason: "auth_check_failed" }, "Error", "auth check failed", "error"],
    [{ phase: "error", reason: "network", detail: "network down" }, "Error", "network down", "error"],
    [{ phase: "error", reason: "websocket_upgrade" }, "Error", "websocket upgrade failed", "error"],
    [{ phase: "error", reason: "websocket_error" }, "Error", "websocket error", "error"],
    [
      { phase: "error", reason: "server_error", detail: "invalid resize payload" },
      "Error",
      "server reported: invalid resize payload",
      "error",
    ],
    [{ phase: "closed", reason: "remote_exit", exitCode: 0 }, "Closed", "remote exited 0", "closed"],
  ] satisfies Array<[ConnectionState, string, string, string]>)("maps %#", (state, primary, detail, tone) => {
      expect(connectionStatusView(state)).toMatchObject({ primary, detail, tone });
    });

  it("lets the new-session notice override the ordinary connected detail", () => {
    expect(connectionStatusView({ phase: "connected", notice: "new_session" })).toMatchObject({
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
    expect(normalizeStatusDetail(" first line\nsecond line\tthird line ")).toBe("first line second line third line");
    expect(normalizeStatusDetail("x".repeat(160))).toHaveLength(120);
  });

  it("redacts credential-like details", () => {
    expect(normalizeStatusDetail("Authorization: Basic abc123")).toBe("[redacted]");
    expect(normalizeStatusDetail("Basic abc123")).toBe("[redacted]");
    expect(normalizeStatusDetail("credential user:secret rejected")).toBe("[redacted]");
    expect(normalizeStatusDetail("credentials user:secret rejected")).toBe("[redacted]");
  });
});
