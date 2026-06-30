import { useCallback, useEffect, useRef, useState } from "react";

import ConnectionStatusBar from "./terminal/ConnectionStatusBar";
import GhosttyTerminal, { type TerminalHandle } from "./terminal/GhosttyTerminal";
import { NEW_SESSION_NOTICE_MS, type ConnectionState } from "./terminal/connectionState";
import { decodeServerFrame, encodeInput, encodeResize } from "./terminal/protocol";
import { resolveAuthCheckUrl, resolveWebSocketUrl } from "./terminal/urls";

const RECONNECT_DELAY_MS = 1000;

export default function App() {
  const terminalRef = useRef<TerminalHandle>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latestSizeRef = useRef<{ cols: number; rows: number } | null>(null);
  const [connectionState, setConnectionState] = useState<ConnectionState>({ phase: "connecting" });

  const sendInput = useCallback((data: string) => {
    const socket = socketRef.current;
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(encodeInput(data));
    }
  }, []);

  const sendResize = useCallback((cols: number, rows: number) => {
    latestSizeRef.current = { cols, rows };
    const socket = socketRef.current;
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(encodeResize(cols, rows));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let attempt = 0;
    let remoteExited = false;
    let noticeTimer: ReturnType<typeof setTimeout> | null = null;

    const clearReconnectTimer = () => {
      if (reconnectTimerRef.current) {
        clearTimeout(reconnectTimerRef.current);
        reconnectTimerRef.current = null;
      }
    };

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

    const scheduleReconnect = () => {
      if (cancelled || reconnectTimerRef.current) {
        return;
      }
      clearNoticeTimer();
      setConnectionState((current) => {
        if (current.phase === "reconnecting" && current.reason === "network") {
          return current;
        }
        return { phase: "reconnecting" };
      });
      reconnectTimerRef.current = setTimeout(() => {
        reconnectTimerRef.current = null;
        void connect(true);
      }, RECONNECT_DELAY_MS);
    };

    async function connect(isReconnect: boolean) {
      const currentAttempt = ++attempt;
      setState({ phase: isReconnect ? "reconnecting" : "connecting" });

      let auth: Response;
      try {
        auth = await fetch(resolveAuthCheckUrl(), { credentials: "same-origin" });
      } catch (error: unknown) {
        if (cancelled || currentAttempt !== attempt) {
          return;
        }
        if (isReconnect) {
          setState({
            phase: "reconnecting",
            reason: "network",
            detail: error instanceof Error ? error.message : "connection error",
          });
          scheduleReconnect();
        } else {
          setState({
            phase: "error",
            reason: "network",
            detail: error instanceof Error ? error.message : "connection error",
          });
        }
        return;
      }
      if (cancelled || currentAttempt !== attempt) {
        return;
      }
      if (auth.status === 401) {
        setState({ phase: "error", reason: "auth_required" });
        return;
      }
      if (auth.status !== 204) {
        setState({ phase: "error", reason: "auth_check_failed" });
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
        showConnected(isReconnect);
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
        if (remoteExited) {
          return;
        }
        if (opened) {
          scheduleReconnect();
        } else if (isReconnect) {
          scheduleReconnect();
        } else {
          setState({ phase: "error", reason: "websocket_upgrade" });
        }
      });
      socket.addEventListener("error", () => {
        if (cancelled || currentAttempt !== attempt) {
          return;
        }
        if (!opened && !isReconnect) {
          setState({ phase: "error", reason: "websocket_error" });
        }
      });
      socket.addEventListener("message", (event) => {
        if (cancelled || currentAttempt !== attempt) {
          return;
        }
        const data = event.data instanceof ArrayBuffer
          ? new Uint8Array(event.data)
          : new TextEncoder().encode(String(event.data));
        const frame = decodeServerFrame(data);
        if (frame.type === "output") {
          terminalRef.current?.write(frame.data);
        } else if (frame.type === "error") {
          setState({ phase: "error", reason: "server_error", detail: frame.message });
        } else {
          remoteExited = true;
          setState({ phase: "closed", reason: "remote_exit", exitCode: frame.code });
          socket.close();
        }
      });
    }

    connect(false).catch((error: unknown) => {
      setState({
        phase: "error",
        reason: "network",
        detail: error instanceof Error ? error.message : "connection error",
      });
    });

    return () => {
      cancelled = true;
      attempt += 1;
      clearReconnectTimer();
      clearNoticeTimer();
      socketRef.current?.close();
      socketRef.current = null;
    };
  }, []);

  return (
    <main className="terminal-shell" data-status={connectionState.phase}>
      <ConnectionStatusBar state={connectionState} />
      <GhosttyTerminal ref={terminalRef} onData={sendInput} onResize={sendResize} />
    </main>
  );
}
