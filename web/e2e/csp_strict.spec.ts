import { expect, test } from "@playwright/test";
import { createServer, type Server } from "node:http";
import { existsSync, readFileSync } from "node:fs";
import { extname, join, resolve } from "node:path";

// Regression: an inline <script> in index.html was blocked by the production CSP and logged a
// violation on every load. Serves the production build with the Content-Security-Policy taken
// verbatim from deploy/public/Caddyfile and requires zero CSP violations and no inline script.

const dist = resolve(import.meta.dirname, "../.e2e-dist-csp");
const caddyfile = readFileSync(resolve(import.meta.dirname, "../../deploy/public/Caddyfile"), "utf8");
const policyTemplate = /Content-Security-Policy "([^"]+)"/.exec(caddyfile)![1];
const types: Record<string, string> = {
  ".html": "text/html", ".js": "text/javascript", ".css": "text/css", ".wasm": "application/wasm",
  ".svg": "image/svg+xml", ".png": "image/png", ".json": "application/json",
};

let server: Server;
let origin: string;

test.beforeAll(async () => {
  server = createServer((req, res) => {
    const path = new URL(req.url ?? "/", "http://x").pathname;
    if (/^\/(api|ws|advisor|health)(\/|$)/.test(path)) return void res.writeHead(404).end();
    let file = join(dist, path);
    if (!file.startsWith(dist) || !existsSync(file) || path.endsWith("/")) file = join(dist, "index.html");
    const csp = policyTemplate.replace("{host}", (req.headers.host ?? "") as string);
    res.writeHead(200, { "content-type": types[extname(file)] ?? "application/octet-stream", "content-security-policy": csp });
    res.end(readFileSync(file));
  });
  await new Promise<void>((done) => server.listen(0, "127.0.0.1", done));
  origin = `http://127.0.0.1:${(server.address() as { port: number }).port}`;
});
test.afterAll(() => server.close());

test("built index.html has no inline script", () => {
  const html = readFileSync(join(dist, "index.html"), "utf8");
  for (const [, attrs, body] of html.matchAll(/<script\b([^>]*)>([\s\S]*?)<\/script>/gi)) {
    expect(attrs).toMatch(/\bsrc=/);
    expect(body.trim()).toBe("");
  }
});

for (const path of ["/", "/games/abc123", "/games/abc123#seat=red&token=t0k"]) {
  test(`loads ${path} under the production CSP without violations`, async ({ page }) => {
    const messages: string[] = [];
    page.on("console", (m) => messages.push(`${m.type()}: ${m.text()}`));
    page.on("pageerror", (e) => messages.push(`pageerror: ${e.message}`));
    await page.goto(origin + path);
    await expect(page.locator("#root")).not.toBeEmpty();
    await page.waitForTimeout(500);
    expect(messages.filter((m) => /content security policy|violates|csp/i.test(m))).toEqual([]);
  });
}
