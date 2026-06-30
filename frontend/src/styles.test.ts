import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const styles = readFileSync(join(process.cwd(), "src/styles.css"), "utf8");

describe("terminal styles", () => {
  it("adds viewport breathing room around the terminal", () => {
    expect(styles).toContain("margin: 6px 8px");
  });

  it("hides the browser caret for ghostty's contenteditable host", () => {
    expect(styles).toMatch(/\.terminal\s*\{[^}]*caret-color:\s*transparent;/s);
  });

  it("hides the browser caret for ghostty's hidden input", () => {
    expect(styles).toContain(".terminal textarea");
    expect(styles).toContain("caret-color: transparent");
  });

  it("keeps the status bar single-line with stable height", () => {
    expect(styles).toMatch(/\.status\s*\{[^}]*min-height:\s*24px;/s);
    expect(styles).toMatch(/\.status\s*\{[^}]*white-space:\s*nowrap;/s);
    expect(styles).toMatch(/\.status\s*\{[^}]*overflow:\s*hidden;/s);
  });

  it("styles primary and detail status text separately", () => {
    expect(styles).toContain(".status__primary");
    expect(styles).toContain(".status__separator");
    expect(styles).toContain(".status__detail");
    expect(styles).toMatch(/\.status__detail\s*\{[^}]*text-overflow:\s*ellipsis;/s);
  });

  it("uses semantic data attributes for status tones", () => {
    expect(styles).toContain('.status[data-phase="connected"]');
    expect(styles).toContain('.status[data-phase="reconnecting"]');
    expect(styles).toContain('.status[data-phase="error"]');
    expect(styles).toContain('.status[data-phase="closed"]');
  });
});
