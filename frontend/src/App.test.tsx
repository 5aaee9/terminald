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

function decodeResize(frame: unknown) {
  const data = frame as Uint8Array;
  return JSON.parse(new TextDecoder().decode(data.slice(1))) as {
    cols: number;
    rows: number;
  };
}

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

describe("App", () => {
  it("shows connecting then connected", async () => {
    render(<App />);
    expect(screen.getByRole("status")).toHaveTextContent("Connecting · auth check pending");
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].open();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Connected · ws ready");
    });
  });

  it("does not open websocket when auth fails", async () => {
    vi.stubGlobal("fetch", vi.fn(async () => new Response(null, { status: 401 })));
    render(<App />);
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Auth required · credentials rejected");
    });
    expect(sockets).toHaveLength(0);
  });

  it("renders reconnecting and error states", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].open();
    sockets[0].closeEvent();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Reconnecting · retrying connection");
    });

    cleanup();
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(2));
    sockets[1].fail();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Error · websocket error");
    });
  });

  it("reports immediate websocket close after auth check", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].closeEvent();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Error · websocket upgrade failed");
    });
  });

  it("sends input and resize frames", async () => {
    render(<App />);
    await waitFor(() => expect(sockets).toHaveLength(1));
    sockets[0].open();
    await waitFor(() => {
      expect(screen.getByRole("status")).toHaveTextContent("Connected · ws ready");
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

  it("reconnects after an established websocket closes", async () => {
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    expect(fetchMock).toHaveBeenCalledTimes(1);
    sockets[0].open();

    sockets[0].closeEvent();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting · retrying connection");

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    expect(fetchMock).toHaveBeenCalledTimes(2);
    sockets[1].open();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Connected · new PTY session");
  });

  it("stops reconnecting when the remote command exits", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();

    sockets[0].message(new Uint8Array([4, 45, 49]));
    sockets[0].closeEvent();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Closed · remote exited -1");

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(1);
  });

  it("sends the latest terminal size when a reconnect opens", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    screen.getByRole("button", { name: "terminal" }).click();
    expect(decodeResize(sockets[0].sent[1])).toEqual({ cols: 80, rows: 24 });

    sockets[0].closeEvent();
    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    sockets[1].open();
    expect(sockets[1].sent).toHaveLength(1);
    expect((sockets[1].sent[0] as Uint8Array)[0]).toBe(0);
    expect(decodeResize(sockets[1].sent[0])).toEqual({ cols: 80, rows: 24 });
  });

  it("cancels pending reconnect when unmounted", async () => {
    vi.useFakeTimers();
    const view = render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].closeEvent();

    view.unmount();
    await advanceReconnectDelay();

    expect(sockets).toHaveLength(1);
  });

  it("does not retry an immediate websocket close before open", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].closeEvent();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Error · websocket upgrade failed");

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(1);
  });

  it("keeps retrying when a reconnect websocket closes before open", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].closeEvent();

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
    sockets[1].closeEvent();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting · retrying connection");

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(3);
  });

  it("reconnects after an established websocket error is followed by close", async () => {
    vi.useFakeTimers();
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    sockets[0].fail();
    sockets[0].closeEvent();

    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting · retrying connection");
    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
  });

  it("stops reconnecting when auth is rejected during retry", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 401 }));

    sockets[0].closeEvent();
    await advanceReconnectDelay();

    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Auth required · credentials rejected");
    expect(sockets).toHaveLength(1);
  });

  it("keeps retrying when auth check fetch fails during reconnect", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    fetchMock.mockRejectedValueOnce(new Error("network down"));

    sockets[0].closeEvent();
    await advanceReconnectDelay();
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Reconnecting · network down");

    await advanceReconnectDelay();

    expect(sockets).toHaveLength(2);
  });

  it("stops reconnecting when auth check fails during retry", async () => {
    vi.useFakeTimers();
    const fetchMock = vi.fn(async () => new Response(null, { status: 204 }));
    vi.stubGlobal("fetch", fetchMock);
    render(<App />);
    await flushEffects();
    expect(sockets).toHaveLength(1);
    sockets[0].open();
    fetchMock.mockResolvedValueOnce(new Response(null, { status: 500 }));

    sockets[0].closeEvent();
    await advanceReconnectDelay();

    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Error · auth check failed");
    expect(sockets).toHaveLength(1);

    await advanceReconnectDelay();
    expect(sockets).toHaveLength(1);
  });

  it("reports initial auth check fetch failure without retrying", async () => {
    vi.useFakeTimers();
    vi.stubGlobal("fetch", vi.fn(async () => {
      throw new Error("network down");
    }));

    render(<App />);
    await flushEffects();
    expect(screen.getByRole("status")).toHaveTextContent("Error · network down");

    await advanceReconnectDelay();
    expect(sockets).toHaveLength(0);
  });
});
