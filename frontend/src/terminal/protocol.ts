const CLIENT_RESIZE = 0;
const CLIENT_INPUT = 1;
const SERVER_OUTPUT = 2;
const SERVER_ERROR = 3;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export type ServerFrame =
  | { type: "output"; data: Uint8Array }
  | { type: "error"; message: string };

export function encodeInput(input: string | Uint8Array): Uint8Array {
  const data = typeof input === "string" ? encoder.encode(input) : input;
  const frame = new Uint8Array(data.length + 1);
  frame[0] = CLIENT_INPUT;
  frame.set(data, 1);
  return frame;
}

export function encodeResize(cols: number, rows: number): Uint8Array {
  const payload = encoder.encode(JSON.stringify({ cols, rows }));
  const frame = new Uint8Array(payload.length + 1);
  frame[0] = CLIENT_RESIZE;
  frame.set(payload, 1);
  return frame;
}

export function decodeServerFrame(frame: Uint8Array): ServerFrame {
  const prefix = frame[0];
  const payload = frame.slice(1);
  if (prefix === SERVER_OUTPUT) {
    return { type: "output", data: payload };
  }
  if (prefix === SERVER_ERROR) {
    return { type: "error", message: decoder.decode(payload) };
  }
  throw new Error(`unknown server frame prefix ${prefix}`);
}
