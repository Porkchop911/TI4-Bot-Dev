#!/usr/bin/env node
// npm run screenshots -- <folder|all>
// Runs every capture-*.ts in the artifact folder(s) with Playwright (which starts and stops the
// vite dev server itself), then builds each folder's index.html from its manifest.json.
import { spawnSync } from "node:child_process";
import { readdirSync, existsSync } from "node:fs";
import { dirname, join, resolve, basename } from "node:path";
import { fileURLToPath } from "node:url";

const root = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const web = resolve(root, "../..");
const arg = process.argv[2] ?? "all";
const folders = readdirSync(root, { withFileTypes: true })
  .filter((d) => d.isDirectory() && /^[A-Z]{1,2}-/.test(d.name))
  .map((d) => d.name)
  .filter((name) => arg === "all" || name === arg || name.startsWith(`${arg}-`) || basename(arg) === name);
if (!folders.length) {
  console.error(`no artifact folder matches "${arg}"`);
  process.exit(2);
}
let failed = false;
for (const name of folders) {
  console.log(`\n== ${name}`);
  const run = spawnSync(
    "npx",
    ["playwright", "test", "-c", "e2e/screenshots/playwright.config.ts", `e2e/screenshots/${name}/`, "--reporter=list"],
    { cwd: web, stdio: "inherit" },
  );
  if (run.status !== 0) {
    failed = true;
    console.error(`captures failed in ${name}`);
    continue;
  }
  if (existsSync(join(root, name, "manifest.json")))
    spawnSync("node", [join(root, "_shared/build-artifact.mjs"), join(root, name)], { stdio: "inherit" });
}
process.exit(failed ? 1 : 0);
