import { readFileSync, readdirSync } from "node:fs";
import { join, relative, resolve, dirname } from "node:path";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

// The layers of web2. A layer may import only the layers listed for it, so that the real
// components (`game`) never see the dummy (`mock`).
const ALLOWED: Record<string, string[]> = {
  ui: [],
  model: [],
  game: ["ui", "model"],
  mock: ["model", "ui"],
  // The game from a transport (the wasm engine, or the server), as the view models.
  session: ["model"],
  dev: ["ui", "model", "game", "mock", "session"],
};

const root = dirname(fileURLToPath(import.meta.url));
const files = (dir: string): string[] =>
  readdirSync(dir, { withFileTypes: true }).flatMap((entry) =>
    entry.isDirectory()
      ? files(join(dir, entry.name))
      : /\.tsx?$/.test(entry.name)
        ? [join(dir, entry.name)]
        : [],
  );

// The protocol fixtures of both apps (`fixtures/protocol/` of the repository), and the test server
// of `web-shared/test/`, which tests read.
const fixtures = resolve(root, "../../fixtures/protocol");
const testServer = resolve(root, "../../web-shared/test/server.ts");

it.each(Object.keys(ALLOWED))("%s imports only its allowed layers", (layer) => {
  const bad: string[] = [];
  for (const file of files(join(root, layer))) {
    for (const [, target] of readFileSync(file, "utf8").matchAll(/from "(\.[^"]+)"/g)) {
      const path = resolve(dirname(file), target);
      if (/\.test\.tsx?$/.test(file) && (dirname(path) === fixtures || path === testServer)) {
        continue;
      }
      const to = relative(root, path).split("/")[0];
      if (to !== layer && !ALLOWED[layer].includes(to)) {
        bad.push(`${relative(root, file)} -> ${target}`);
      }
    }
  }
  expect(bad).toEqual([]);
});

// The module shared with web/ (`web-shared/AGENTS.md`). Only `session` imports it.
it.each(Object.keys(ALLOWED))("%s imports @ti4/shared only if it is session", (layer) => {
  const bad = files(join(root, layer)).filter(
    (file) => layer !== "session" && /from "@ti4\/shared[/"]/.test(readFileSync(file, "utf8")),
  );
  expect(bad.map((file) => relative(root, file))).toEqual([]);
});

// `web-shared` imports no app, and no package but `node:` and vitest, and `fake-indexeddb` in
// its tests (a new dependency needs both maintainers).
it("web-shared imports no app", () => {
  const shared = resolve(root, "../../web-shared");
  const bad: string[] = [];
  for (const file of files(shared)) {
    const test = file.endsWith(".test.ts");
    for (const [, target] of readFileSync(file, "utf8").matchAll(
      /(?:from|import\(|^import) ?"([^"]+)"/gm,
    )) {
      const outside = target.startsWith(".")
        ? relative(shared, resolve(dirname(file), target)).startsWith("..")
        : !/^(node:|vitest$)/.test(target) && !(test && /^fake-indexeddb(\/|$)/.test(target));
      if (outside) {
        bad.push(`${relative(shared, file)} -> ${target}`);
      }
    }
  }
  expect(bad).toEqual([]);
});
