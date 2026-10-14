// @vitest-environment node
import { execFileSync } from "node:child_process";
import { expect, test } from "vitest";
import { mkdtempSync, mkdirSync, realpathSync, rmSync, symlinkSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import path from "node:path";

const webDir = path.resolve(__dirname, "../..");

function fakePackage(dir: string, name: string) {
  mkdirSync(dir, { recursive: true });
  writeFileSync(path.join(dir, "package.json"), JSON.stringify({ name, version: "1.0.0", main: "index.js" }));
  writeFileSync(path.join(dir, "index.js"), "module.exports = {};\n");
}

// Vite's native bindings do not work inside the vmThreads pool, so the resolution runs in a plain node child.
const resolveScript = `
import { createServer, loadConfigFromFile } from "vite";
const [configFile, root, importer] = process.argv.slice(1);
const loaded = await loadConfigFromFile({ command: "serve", mode: "development" }, configFile, undefined, "silent");
const server = await createServer({
  configFile: false,
  root,
  logLevel: "silent",
  resolve: loaded?.config.resolve,
  server: { middlewareMode: true, ws: false },
  optimizeDeps: { noDiscovery: true, include: [] },
});
const resolved = await server.environments.client.pluginContainer.resolveId("react", importer);
await server.close();
process.stdout.write(resolved?.id ?? "");
`;

// Nightly runs s1-05-2103 .. s1-13-2113: a stray web/node_modules/node_modules link to another checkout's
// node_modules made Vite pre-bundle react-dom against that checkout's React while the app used its own,
// so every page died with "Invalid hook call" before seat 1 rendered. The config must pin React to the root.
test("vite config resolves react from react-dom to the project's own copy despite a nested node_modules link", () => {
  const root = realpathSync(mkdtempSync(path.join(tmpdir(), "vite-dedupe-")));
  const other = path.join(root, "other-checkout", "node_modules");
  try {
    fakePackage(path.join(root, "node_modules", "react"), "react");
    fakePackage(path.join(root, "node_modules", "react-dom"), "react-dom");
    fakePackage(path.join(other, "react"), "react");
    symlinkSync(other, path.join(root, "node_modules", "node_modules"));
    writeFileSync(path.join(root, "package.json"), JSON.stringify({ name: "fixture", private: true }));

    const resolved = execFileSync(
      process.execPath,
      [
        "--input-type=module",
        "-e",
        resolveScript,
        path.join(webDir, "vite.config.ts"),
        root,
        path.join(root, "node_modules", "react-dom", "index.js"),
      ],
      { cwd: webDir, encoding: "utf8" },
    );
    expect(resolved).toBe(path.join(root, "node_modules", "react", "index.js"));
  } finally {
    rmSync(root, { recursive: true, force: true });
  }
}, 30_000);
