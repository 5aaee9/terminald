import { useCallback, useEffect, useRef, useState } from "react";

import GhosttyTerminal, { type TerminalHandle } from "./terminal/GhosttyTerminal";
import { decodeServerFrame, encodeInput, encodeResize } from "./terminal/protocol";
import { resolveAuthCheckUrl, resolveWebSocketUrl } from "./terminal/urls";

type Status = "connecting" | "connected" | "closed" | "error";

export default function App() {
  const terminalRef = useRef<TerminalHandle>(null);
  const socketRef = useRef<WebSocket | null>(null);
  const [status, setStatus] = useState<Status>("connecting");
  const [message, setMessage] = useState("connecting");

  const sendInput = useCallback((data: string) => {
    const socket = socketRef.current;
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(encodeInput(data));
    }
  }, []);

  const sendResize = useCallback((cols: number, rows: number) => {
    const socket = socketRef.current;
    if (socket?.readyState === WebSocket.OPEN) {
      socket.send(encodeResize(cols, rows));
    }
  }, []);

  useEffect(() => {
    let cancelled = false;
    let opened = false;

    async function connect() {
      setStatus("connecting");
      setMessage("connecting");
      const auth = await fetch(resolveAuthCheckUrl(), { credentials: "same-origin" });
      if (cancelled) {
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

      socket.addEventListener("open", () => {
        opened = true;
        setStatus("connected");
        setMessage("connected");
      });
      socket.addEventListener("close", () => {
        socketRef.current = null;
        if (opened) {
          setStatus("closed");
          setMessage("closed");
        } else {
          setStatus("error");
          setMessage("authentication or websocket upgrade failed");
        }
      });
      socket.addEventListener("error", () => {
        setStatus("error");
        setMessage("websocket error");
      });
      socket.addEventListener("message", (event) => {
        const data = event.data instanceof ArrayBuffer
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

    connect().catch((error: unknown) => {
      setStatus("error");
      setMessage(error instanceof Error ? error.message : "connection error");
    });

    return () => {
      cancelled = true;
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
