import { render, waitFor } from "@testing-library/react";
import { createRef } from "react";
import { beforeEach, describe, expect, it, vi } from "vitest";

import GhosttyTerminal, { type TerminalHandle } from "./GhosttyTerminal";

const terminal = {
  cols: 80,
  rows: 24,
  dispose: vi.fn(),
  focus: vi.fn(),
  loadAddon: vi.fn(),
  onData: vi.fn(),
  open: vi.fn(),
  write: vi.fn(),
};

vi.mock("ghostty-web", () => ({
  init: vi.fn(async () => undefined),
  Terminal: vi.fn(function Terminal() {
    return terminal;
  }),
  FitAddon: vi.fn(function FitAddon() {
    return { fit: vi.fn() };
  }),
}));

beforeEach(() => {
  vi.clearAllMocks();
  vi.stubGlobal(
    "ResizeObserver",
    class {
      observe = vi.fn();
      disconnect = vi.fn();
    }
  );
});

describe("GhosttyTerminal", () => {
  it("initializes ghostty before opening terminal", async () => {
    render(<GhosttyTerminal onData={vi.fn()} onResize={vi.fn()} />);
    await waitFor(() => expect(terminal.open).toHaveBeenCalled());
  });

  it("exposes write through the adapter", async () => {
    const ref = createRef<TerminalHandle>();
    render(<GhosttyTerminal ref={ref} onData={vi.fn()} onResize={vi.fn()} />);
    await waitFor(() => expect(ref.current).not.toBeNull());
    ref.current?.write(new Uint8Array([1]));
    expect(terminal.write).toHaveBeenCalledWith(new Uint8Array([1]));
  });
});
