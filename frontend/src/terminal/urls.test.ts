import { describe, expect, it } from "vitest";

import { resolveAuthCheckUrl, resolveWebSocketUrl } from "./urls";

describe("terminal URLs", () => {
  it("resolves websocket URLs under a mount", () => {
    expect(resolveWebSocketUrl("http://site.com/aaa/")).toBe(
      "ws://site.com/aaa/ws"
    );
  });

  it("resolves auth check URLs under nested mounts", () => {
    expect(resolveAuthCheckUrl("https://site.com/example/bbb/")).toBe(
      "https://site.com/example/bbb/auth/check"
    );
  });

  it("normalizes missing trailing slash", () => {
    expect(resolveWebSocketUrl("http://site.com/aaa")).toBe(
      "ws://site.com/aaa/ws"
    );
  });
});
