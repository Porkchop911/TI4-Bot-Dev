// The size gate of the browser build. Prints the size of the wasm file (raw, gzip, brotli) and
// fails when one is over its budget in crates/ti4-wasm/size-budget.json.
//
//   cargo build -p ti4-wasm --target wasm32-unknown-unknown --profile wasm-release
//   node scripts/wasm-size-gate.mjs [file.wasm]
//
// Raise a budget on purpose, in the commit that makes the file larger, and say why in the
// message; a file that grows by itself is how a one-megabyte download becomes five.
import { readFileSync, appendFileSync } from "node:fs";
import { brotliCompressSync, constants, gzipSync } from "node:zlib";

const file = process.argv[2] ?? "target/wasm32-unknown-unknown/wasm-release/ti4_wasm.wasm";
const budgetFile = new URL("../crates/ti4-wasm/size-budget.json", import.meta.url);
const budget = JSON.parse(readFileSync(budgetFile, "utf8"));
const bytes = readFileSync(file);

const size = {
  raw: bytes.length,
  gzip: gzipSync(bytes, { level: 9 }).length,
  brotli: brotliCompressSync(bytes, {
    params: {
      [constants.BROTLI_PARAM_QUALITY]: 11,
      [constants.BROTLI_PARAM_SIZE_HINT]: bytes.length,
    },
  }).length,
};

const mb = (n) => (n / 1e6).toFixed(2).padStart(6);
const rows = ["raw", "gzip", "brotli"].map((kind) => {
  const over = size[kind] > budget[kind];
  return { kind, over, line: `${kind.padEnd(7)} ${mb(size[kind])} MB   budget ${mb(budget[kind])} MB   ${over ? "OVER" : "ok"}` };
});
console.log(`${file}\n${rows.map((r) => r.line).join("\n")}`);

if (process.env.GITHUB_STEP_SUMMARY) {
  appendFileSync(
    process.env.GITHUB_STEP_SUMMARY,
    `### ti4-wasm size\n\n| | size | budget |\n|---|---|---|\n${["raw", "gzip", "brotli"]
      .map((kind) => `| ${kind} | ${mb(size[kind])} MB | ${mb(budget[kind])} MB |`)
      .join("\n")}\n`,
  );
}
if (rows.some((r) => r.over)) {
  console.error("The wasm file is over its size budget (crates/ti4-wasm/size-budget.json).");
  process.exit(1);
}
