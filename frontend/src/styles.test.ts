import { describe, expect, it } from "vitest";
import { readFileSync } from "node:fs";
import { join } from "node:path";

const styles = readFileSync(join(process.cwd(), "src/styles.css"), "utf8");

describe("terminal styles", () => {
  it("hides the browser caret for ghostty's hidden input", () => {
    expect(styles).toContain(".terminal textarea");
    expect(styles).toContain("caret-color: transparent");
  });
});
