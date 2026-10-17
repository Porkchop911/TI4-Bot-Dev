import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { describe, expect, it } from "vitest";

// deploy/public/Caddyfile sends `script-src 'self' 'wasm-unsafe-eval'` (no 'unsafe-inline'), so an
// inline <script> in index.html is blocked and logs a CSP violation. Keep every script external.
describe("index.html", () => {
  const html = readFileSync(resolve(process.cwd(), "index.html"), "utf8");
  const scripts = [...html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)];

  it("has an entry script", () => {
    expect(scripts.some(([, attrs]) => /src="\/src\/main\.tsx"/.test(attrs))).toBe(true);
  });

  it("has no inline script", () => {
    for (const [, attrs, body] of scripts) {
      expect(attrs).toMatch(/\bsrc=/);
      expect(body.trim()).toBe("");
    }
  });

  it("has no inline event handlers", () => {
    expect(html).not.toMatch(/\son[a-z]+\s*=/i);
  });
});
