import { cleanup, render, screen, waitFor } from "@testing-library/react";
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
  default: ({ onData, onResize }: { onData(data: string): void; onResize(cols: number, rows: number): void }) => (
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

function decodeResize(frame: unknown) {
  const data = frame as Uint8Array;
  return JSON.parse(new TextDecoder().decode(data.slice(1))) as {
    cols: number;
    rows: number;
  };
}

beforeEach(() => {
  sockets.length = 0;
  vi.stubGlobal("WebSocket", MockSocket);
  vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 204 })));
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("App", () => {
  it("shows connecting then connected", async () => {
    render(<App />);
    expect(screen.getByRole("status")).toHaveTextContent("connecting");
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].open();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("connected");
    });
  });

  it("does not open websocket when auth fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 401 })));
    render(<App />);
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("authentication required");
    });
    expect(sockets).toHaveLength(0);
  });

  it("renders closed and error states", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].open();
    sockets[0].closeEvent();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("closed");
    });

    cleanup();
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(2));
    sockets[1].fail();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("websocket error");
    });
  });

  it("reports immediate websocket close after auth check", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].closeEvent();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("authentication or websocket upgrade failed");
    });
  });

  it("sends input and resize frames", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].open();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("connected");
    });
    screen.getByRole("button", { name: "terminal" }).click();
    expect(sockets[0].sent).toHaveLength(2);
    expect(Array.from(sockets[0].sent[0] as Uint8Array)).toEqual([1, 97]);
    expect((sockets[0].sent[1] as Uint8Array)[0]).toBe(0);
  });

  it("sends the terminal size reported before websocket open once connected", async () => {
    render(<App />);
    screen.getByRole("button", { name: "terminal" }).click();

    await waitFor(() => expect(sockets).toHaveLength(1));
    expect(sockets[0].sent).toHaveLength(0);

    sockets[0].open();

    await waitFor(() => expect(sockets[0].sent).toHaveLength(1));
    expect((sockets[0].sent[0] as Uint8Array)[0]).toBe(0);
    expect(decodeResize(sockets[0].sent[0])).toEqual({ cols: 80, rows: 24 });
  });
});
