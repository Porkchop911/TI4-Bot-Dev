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

it.each(Object.keys(ALLOWED))("%s imports only its allowed layers", (layer) => {
  const bad: string[] = [];
  for (const file of files(join(root, layer))) {
    for (const [, target] of readFileSync(file, "utf8").matchAll(/from "(\.[^"]+)"/g)) {
      const to = relative(root, resolve(dirname(file), target)).split("/")[0];
      if (to !== layer && !ALLOWED[layer].includes(to)) {
        bad.push(`${relative(root, file)} -> ${target}`);
      }
    }
  }
  expect(bad).toEqual([]);
});
