import { useCallback, useEffect, useRef, useState } from "react";

import GhosttyTerminal, { type TerminalHandle } from "./terminal/GhosttyTerminal";
import { decodeServerFrame, encodeInput, encodeResize } from "./terminal/protocol";
import { resolveAuthCheckUrl, resolveWebSocketUrl } from "./terminal/urls";

type Status = "connecting" | "connected" | "reconnecting" | "closed" | "error";

const RECONNECT_DELAY_MS = 1000;

export default function App() {
  const terminalRef = useRef<TerminalHandle>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const reconnectTimerRef = useRef<ReturnType<typeof setTimeout> | null>(null);
  const latestSizeRef = useRef<{ cols: number; rows: number } | null>(null);
  const [status, setStatus] = useState<Status>("connecting");
  const [message, setMessage] = useState("connecting");

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

    async function connect(isReconnect: boolean) {
      const currentAttempt = ++attempt;
      setStatus(isReconnect ? "reconnecting" : "connecting");
      setMessage(isReconnect ? "reconnecting" : "connecting");

      let auth: Response;
      try {
        auth = await fetch(resolveAuthCheckUrl(), { credentials: "same-origin" });
      } catch (error: unknown) {
        if (cancelled || currentAttempt !== attempt) {
          return;
        }
        if (isReconnect) {
          scheduleReconnect();
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
        if (remoteExited) {
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
        const data = event.data instanceof ArrayBuffer
          ? new Uint8Array(event.data)
          : new TextEncoder().encode(String(event.data));
        const frame = decodeServerFrame(data);
        if (frame.type === "output") {
          terminalRef.current?.write(frame.data);
        } else if (frame.type === "error") {
          setStatus("error");
          setMessage(frame.message);
        } else {
          remoteExited = true;
          setStatus("closed");
          setMessage(`Remote exited with code ${frame.code}`);
          socket.close();
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

  return (
    <main className="terminal-shell" data-status={status}>
      <div className="status" role="status">{message}</div>
      <GhosttyTerminal ref={terminalRef} onData={sendInput} onResize={sendResize} />
    </main>
  );
}
