#!/usr/bin/env node
// Builds the offline engine (crates/ti4-wasm) and copies it to src/offline/local/engine/, where
// vite picks it up (hash-named, lazy-loaded; see src/offline/local/engineAsset.ts).
//
//   node scripts/build-wasm.mjs           cargo build --profile wasm-release, then copy
//   TI4_WASM_FILE=<file> node ...         copy a prebuilt file (CI, the e2e build)
//   TI4_SKIP_WASM=1                       do nothing: the build has no "Play offline"
//
// Without cargo or the wasm32 target the script warns and exits 0, so a plain `npm run build`
// still works (the result simply has no offline mode). TI4_REQUIRE_WASM=1 makes that an error.
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const web = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const repo = resolve(web, "..");
const out = join(web, "src/offline/local/engine/ti4_wasm.wasm");
const required = process.env.TI4_REQUIRE_WASM === "1";

function giveUp(message) {
  console.warn(`build-wasm: ${message}`);
  if (required) process.exit(1);
  rmSync(out, { force: true });
  console.warn("build-wasm: continuing without the offline engine");
  process.exit(0);
}

if (process.env.TI4_SKIP_WASM === "1") giveUp("TI4_SKIP_WASM=1");

let built = process.env.TI4_WASM_FILE;
if (!built) {
  const targetDir = process.env.CARGO_TARGET_DIR ?? join(repo, "target");
  const run = spawnSync(
    "cargo",
    ["build", "-p", "ti4-wasm", "--target", "wasm32-unknown-unknown", "--profile", "wasm-release"],
    { cwd: repo, stdio: "inherit", env: process.env },
  );
  if (run.error || run.status !== 0) giveUp("cargo build of ti4-wasm failed (is the wasm32-unknown-unknown target installed?)");
  built = join(targetDir, "wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm");
}
if (!existsSync(built)) giveUp(`${built} does not exist`);
mkdirSync(dirname(out), { recursive: true });
copyFileSync(built, out);
console.log(`build-wasm: ${built} -> ${out} (${(statSync(out).size / 1e6).toFixed(2)} MB)`);
