export const NEW_SESSION_NOTICE_MS = 2000;
const MAX_DETAIL_LENGTH = 120;

export type ConnectionPhase = "connecting" | "connected" | "reconnecting" | "closed" | "error";

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

export type ConnectionStatusTone = "connecting" | "connected" | "reconnecting" | "closed" | "error";

export interface ConnectionStatusView {
  primary: string;
  detail?: string;
  tone: ConnectionStatusTone;
}

export function normalizeStatusDetail(detail: string): string {
  const normalized = detail.replace(/\s+/g, " ").trim();
  if (/authorization\s*:/i.test(normalized) || /\bbasic\s+\S+/i.test(normalized) || /\bcredentials?\b/i.test(normalized)) {
    return "[redacted]";
  }
  return normalized.length > MAX_DETAIL_LENGTH ? normalized.slice(0, MAX_DETAIL_LENGTH) : normalized;
}

export function connectionStatusView(state: ConnectionState): ConnectionStatusView {
  if (state.phase === "connected" && state.notice === "new_session") {
    return { primary: "Connected", detail: "new PTY session", tone: "connected" };
  }

  switch (state.phase) {
    case "connecting":
      return { primary: "Connecting", detail: "auth check pending", tone: "connecting" };
    case "connected":
      return { primary: "Connected", detail: state.detail ? normalizeStatusDetail(state.detail) : "ws ready", tone: "connected" };
    case "reconnecting":
      return {
        primary: "Reconnecting",
        detail: state.detail ? normalizeStatusDetail(state.detail) : "retrying connection",
        tone: "reconnecting",
      };
    case "closed":
      if (state.reason === "remote_exit" && typeof state.exitCode === "number") {
        return { primary: "Closed", detail: `remote exited ${state.exitCode}`, tone: "closed" };
      }
      return { primary: "Closed", detail: state.detail ? normalizeStatusDetail(state.detail) : "connection closed", tone: "closed" };
    case "error":
      return errorStatusView(state);
  }
}

function errorStatusView(state: ConnectionState): ConnectionStatusView {
  if (state.reason === "auth_required") {
    return { primary: "Auth required", detail: "credentials rejected", tone: "error" };
  }
  if (state.reason === "auth_check_failed") {
    return { primary: "Error", detail: "auth check failed", tone: "error" };
  }
  if (state.reason === "network") {
    return { primary: "Error", detail: state.detail ? normalizeStatusDetail(state.detail) : "network failure", tone: "error" };
  }
  if (state.reason === "websocket_upgrade") {
    return { primary: "Error", detail: "websocket upgrade failed", tone: "error" };
  }
  if (state.reason === "websocket_error") {
    return { primary: "Error", detail: state.detail ? normalizeStatusDetail(state.detail) : "websocket error", tone: "error" };
  }
  if (state.reason === "server_error") {
    const detail = state.detail ? normalizeStatusDetail(state.detail) : "unknown error";
    return { primary: "Error", detail: `server reported: ${detail}`, tone: "error" };
  }
  return { primary: "Error", detail: state.detail ? normalizeStatusDetail(state.detail) : "connection error", tone: "error" };
}
