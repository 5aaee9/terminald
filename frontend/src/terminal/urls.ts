export function resolveWebSocketUrl(pageUrl = window.location.href): string {
  const url = new URL("ws", ensureTrailingSlash(pageUrl));
  url.protocol = url.protocol === "https:" ? "wss:" : "ws:";
  return url.toString();
}

export function resolveAuthCheckUrl(pageUrl = window.location.href): string {
  return new URL("auth/check", ensureTrailingSlash(pageUrl)).toString();
}

function ensureTrailingSlash(value: string): string {
  const url = new URL(value);
  if (!url.pathname.endsWith("/")) {
    url.pathname = `${url.pathname}/`;
  }
  return url.toString();
}
