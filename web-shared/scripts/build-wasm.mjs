#!/usr/bin/env node
// Builds the engine (crates/ti4-wasm) for the browser and copies it to the file that an app
// imports. web (`web/scripts/build-wasm.mjs`) and web2 (`web2/scripts/build-wasm.sh`) call it with
// their file; both run the same cargo command, so they share one build.
//
//   node web-shared/scripts/build-wasm.mjs <out.wasm>         cargo build --profile wasm-release, then copy
//   node web-shared/scripts/build-wasm.mjs <out.wasm> --dev   --profile wasm-dev: not optimized, incremental
//   TI4_WASM_FILE=<file> node ...                             copy a prebuilt file (CI, the e2e build)
//   TI4_SKIP_WASM=1                                           do nothing: the build has no engine
//
// Without cargo or the wasm32 target the script warns, removes <out.wasm> and exits 0, so a plain
// `npm run build` of web still works (the result simply has no offline mode). TI4_REQUIRE_WASM=1
// makes that an error (exit 1, <out.wasm> kept).
// Needs: rustup target add wasm32-unknown-unknown
import { spawnSync } from "node:child_process";
import { copyFileSync, existsSync, mkdirSync, rmSync, statSync } from "node:fs";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const repo = resolve(dirname(fileURLToPath(import.meta.url)), "../..");
const usage = "usage: node web-shared/scripts/build-wasm.mjs <out.wasm> [--dev]";

const args = process.argv.slice(2);
const dev = args.includes("--dev");
const files = args.filter((arg) => arg !== "--dev");
if (files.length !== 1 || files[0].startsWith("-")) {
  console.error(usage);
  process.exit(2);
}
const out = resolve(files[0]);
const profile = dev ? "wasm-dev" : "wasm-release";
const required = process.env.TI4_REQUIRE_WASM === "1";

function giveUp(message) {
  console.warn(`build-wasm: ${message}`);
  if (required) {
    process.exit(1);
  }
  rmSync(out, { force: true });
  console.warn("build-wasm: continuing without the offline engine");
  process.exit(0);
}

if (process.env.TI4_SKIP_WASM === "1") {
  giveUp("TI4_SKIP_WASM=1");
}

let built = process.env.TI4_WASM_FILE;
if (!built) {
  const targetDir = process.env.CARGO_TARGET_DIR ?? join(repo, "target");
  const run = spawnSync(
    "cargo",
    ["build", "-p", "ti4-wasm", "--target", "wasm32-unknown-unknown", "--profile", profile],
    { cwd: repo, stdio: "inherit", env: process.env },
  );
  if (run.error || run.status !== 0) {
    giveUp("cargo build of ti4-wasm failed (is the wasm32-unknown-unknown target installed?)");
  }
  built = resolve(repo, targetDir, `wasm32-unknown-unknown/${profile}/ti4_wasm.wasm`);
}
if (!existsSync(built)) {
  giveUp(`${built} does not exist`);
}
mkdirSync(dirname(out), { recursive: true });
copyFileSync(built, out);
console.log(`build-wasm: ${built} -> ${out} (${(statSync(out).size / 1e6).toFixed(2)} MB)`);
