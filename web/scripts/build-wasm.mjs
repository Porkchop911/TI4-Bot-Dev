#!/usr/bin/env node
// Builds the offline engine and copies it to src/offline/local/engine/, where vite picks it up
// (hash-named, lazy-loaded; see src/offline/local/engineAsset.ts). The build is the shared one,
// web-shared/scripts/build-wasm.mjs: its TI4_* flags work here.
//
//   node scripts/build-wasm.mjs           cargo build --profile wasm-release, then copy
//   TI4_WASM_FILE=<file> node ...         copy a prebuilt file (CI, the e2e build)
//   TI4_SKIP_WASM=1                       do nothing: the build has no "Play offline"
//
// Without cargo or the wasm32 target the script warns and exits 0, so a plain `npm run build`
// still works (the result simply has no offline mode). TI4_REQUIRE_WASM=1 makes that an error.
import { spawnSync } from "node:child_process";
import { dirname, join, resolve } from "node:path";
import { fileURLToPath } from "node:url";

const web = resolve(dirname(fileURLToPath(import.meta.url)), "..");
const run = spawnSync(
  process.execPath,
  [join(web, "../web-shared/scripts/build-wasm.mjs"), join(web, "src/offline/local/engine/ti4_wasm.wasm")],
  { stdio: "inherit", env: process.env },
);
process.exit(run.status ?? 1);
