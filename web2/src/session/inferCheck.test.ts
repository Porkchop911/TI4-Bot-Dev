import { existsSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { expect, it } from "vitest";

// The MLP forward of the engine in the page (simd128) gives the bits of the native build (SSE):
// the logits of `fixtures/infer/synthetic_logits.json`, which `cargo wire-web` writes, against
// `ti4_infer_check` of the real engine, built by `scripts/build-wasm.sh`.
const WASM = fileURLToPath(new URL("./ti4.wasm", import.meta.url));
const FIXTURE = fileURLToPath(
  new URL("../../../fixtures/infer/synthetic_logits.json", import.meta.url),
);

interface Case {
  request: unknown;
  digest: string;
  logits: number[][];
}

interface Answer {
  backend: string;
  digest: string;
  logits: number[][];
}

interface Exports {
  memory: WebAssembly.Memory;
  ti4_host_in(length: number): number;
  ti4_host_out_ptr(): number;
  ti4_host_out_len(): number;
  ti4_infer_check(length: number): number;
}

it.skipIf(!existsSync(WASM))(
  "the MLP forward of the engine gives the logits of the native build",
  async () => {
    const module = await WebAssembly.compile(readFileSync(WASM));
    const instance = await WebAssembly.instantiate(module, { host: { ask: () => -1 } });
    const exports = instance.exports as unknown as Exports;
    const cases = JSON.parse(readFileSync(FIXTURE, "utf8")) as Case[];
    expect(cases.length).toBeGreaterThan(0);
    for (const { request, digest, logits } of cases) {
      const bytes = new TextEncoder().encode(JSON.stringify(request));
      // Reserved first: the reservation can grow the memory, which detaches an older view.
      const at = exports.ti4_host_in(bytes.length);
      new Uint8Array(exports.memory.buffer).set(bytes, at);
      const status = exports.ti4_infer_check(bytes.length);
      const text = new TextDecoder().decode(
        new Uint8Array(
          exports.memory.buffer,
          exports.ti4_host_out_ptr(),
          exports.ti4_host_out_len(),
        ),
      );
      expect(status, text).toBeGreaterThanOrEqual(0);
      const answer = JSON.parse(text) as Answer;
      expect(answer.backend).toBe("simd128");
      // The same file: the weights of the seed are the same bits.
      expect(answer.digest).toBe(digest);
      expect(answer.logits).toEqual(logits);
    }
  },
);
