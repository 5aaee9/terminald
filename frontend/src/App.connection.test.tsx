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
  vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 204 })));
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
    expect(screen.getByRole("status")).toHaveTextContent("Connected · new PTY session");

    await act(async () => {
      vi.advanceTimersByTime(2000);
      await Promise.resolve();
    });

    expect(screen.getByRole("status")).toHaveTextContent("Connected · ws ready");
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

    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting · retrying connection");
  });

  it("does not update state from a pending new-session notice after unmount", async () => {
    vi.useFakeTimers();
    const consoleError = vi.spyOn(console, "error").mockImplementation(() => undefined);
    const view = render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].closeEvent();

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    sockets[1].open();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Connected · new PTY session");

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

    sockets[0].message(new Uint8Array([3, ...new TextEncoder().encode("bad resize")]));
    await flushEffects();

    expect(screen.getByRole("status")).toHaveTextContent("Error · server reported: bad resize");
    expect(sockets[0].readyState).toBe(MockSocket.OPEN);

    await advanceReconnectDelay();
    expect(sockets).toHaveLength(1);
  });

  it("lets close-before-open win after an initial websocket error", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));

    sockets[0].fail();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Error · websocket error");
    });

    sockets[0].closeEvent();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Error · websocket upgrade failed");
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
      expect(fetchMock).toHaveBeenCalledWith(`${window.location.origin}/prefix/tool/auth/check`, { credentials: "same-origin" });
      expect(sockets[0].url).toBe(`ws://${window.location.host}/prefix/tool/ws`);
    } finally {
      window.history.pushState({}, "", originalPath);
    }
  });
});
