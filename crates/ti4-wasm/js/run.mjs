// Load the wasm, run a seeded game for a fixed number of steps, and print a hash of the state.
// Usage: node run.mjs <file.wasm> [seed] [max_steps] [players]
import { createHash } from "node:crypto";
import { readFileSync } from "node:fs";

const [file, seed = "7", maxSteps = "300", players = "8"] = process.argv.slice(2);
const started = performance.now();
const { instance } = await WebAssembly.instantiate(readFileSync(file), {
  host: { ask: () => 0 },
});
const wasm = instance.exports;
// An export returns a length, negated when the response buffer holds an error text.
const status = wasm.ti4_run_seeded(Number(seed), Number(players), Number(maxSteps));
const text = new TextDecoder().decode(
  new Uint8Array(wasm.memory.buffer, wasm.ti4_response_ptr(), Math.abs(status)),
);
if (status < 0) throw new Error(text);
const result = JSON.parse(text);
console.log(
  `decisions=${result.decisions} round=${result.round} sha256=${createHash("sha256").update(text).digest("hex")}`,
);
console.error(`${Math.round(performance.now() - started)} ms, memory ${wasm.memory.buffer.byteLength >> 20} MiB`);
