import { describe, expect, it } from "vitest";

import { decodeServerFrame, encodeInput, encodeResize } from "./protocol";

describe("terminal protocol", () => {
  it("encodes input frames", () => {
    expect(Array.from(encodeInput("a"))).toEqual([1, 97]);
  });

  it("encodes resize frames", () => {
    const frame = encodeResize(80, 24);
    expect(frame[0]).toBe(0);
    expect(new TextDecoder().decode(frame.slice(1))).toBe(
      '{"cols":80,"rows":24}'
    );
  });

  it("decodes output frames", () => {
    const frame = decodeServerFrame(new Uint8Array([2, 120]));
    expect(frame).toEqual({ type: "output", data: new Uint8Array([120]) });
  });

  it("decodes exit frames", () => {
    const frame = decodeServerFrame(new Uint8Array([4, 45, 49]));
    expect(frame).toEqual({ type: "exited", code: -1 });
  });
});
