import { mkdirSync, existsSync, readFileSync, writeFileSync } from "node:fs";
import { dirname, join } from "node:path";

/** `before` on the base commit (committed PNGs), `after` for the current code. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const OUT = join(dirname(new URL(import.meta.url).pathname), "out");

/** Merges measurements into out/<prefix>-results.json. */
export function record(key: string, value: unknown) {
  mkdirSync(OUT, { recursive: true });
  const file = join(OUT, `${prefix}-results.json`);
  const all = existsSync(file) ? JSON.parse(readFileSync(file, "utf8")) : {};
  all[key] = value;
  writeFileSync(file, JSON.stringify(all, null, 2));
}
