import { cleanup, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";

import ConnectionStatusBar from "./ConnectionStatusBar";

afterEach(() => {
  cleanup();
});

describe("ConnectionStatusBar", () => {
  it("renders primary text before detail in a polite status region", () => {
    render(<ConnectionStatusBar state={{ phase: "connected" }} />);

    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("aria-live", "polite");
    expect(status).toHaveAttribute("data-phase", "connected");
    expect(status).not.toHaveAttribute("data-reason");
    expect(status).toHaveTextContent("Connected · ws ready");

    const primary = status.querySelector(".status__primary");
    const separator = status.querySelector(".status__separator");
    const detail = status.querySelector(".status__detail");
    expect(primary).toHaveTextContent("Connected");
    expect(separator).toHaveTextContent("·");
    expect(detail).toHaveTextContent("ws ready");
    expect(primary?.compareDocumentPosition(separator as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
    expect(separator?.compareDocumentPosition(detail as Node)).toBe(Node.DOCUMENT_POSITION_FOLLOWING);
  });

  it("sets semantic reason and notice attributes", () => {
    render(<ConnectionStatusBar state={{ phase: "connected", notice: "new_session" }} />);

    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("data-phase", "connected");
    expect(status).toHaveAttribute("data-notice", "new_session");
    expect(status).toHaveTextContent("Connected · new PTY session");
  });

  it("sets semantic reason attributes for error states", () => {
    render(<ConnectionStatusBar state={{ phase: "error", reason: "server_error", detail: "bad resize" }} />);

    const status = screen.getByRole("status");
    expect(status).toHaveAttribute("data-phase", "error");
    expect(status).toHaveAttribute("data-reason", "server_error");
    expect(status).toHaveTextContent("Error · server reported: bad resize");
  });
});
