// Timing of a local game in Chromium: the random seats between two answers, a replay up to
// answer n, an undo from a checkpoint, and the rewrite of a saved game in localStorage.
// Usage (from web2/): node ../crates/ti4-wasm/js/bench.mjs <file.wasm> [seed] [players] [humans]
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(`${process.cwd()}/`);
const { chromium } = require("@playwright/test");

const [file, seed = "3", players = "8", humans = "1"] = process.argv.slice(2);
const wasm = readFileSync(file).toString("base64");
const browser = await chromium.launch();
const page = await browser.newPage();
// localStorage needs an origin, so the page is served from a made-up one.
await page.route("http://bench.test/", (route) =>
  route.fulfill({ contentType: "text/html", body: "<title>bench</title>" }),
);
await page.goto("http://bench.test/");
const report = await page.evaluate(
  async ({ base64, seed, players, humans }) => {
    const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
    let exports;
    const decoder = new TextDecoder();
    const text = (pointer, length) =>
      decoder.decode(new Uint8Array(exports.memory.buffer, pointer, length));
    // One game. `read` says whether each ask parses its update, as a replay must.
    const play = async (read) => {
      const gaps = [];
      const answers = [];
      let inAsk = 0;
      // An undo at these answers: the time until the choice before is asked again.
      const undoAt = new Set([50, 150, 290]);
      const undos = {};
      let undone = null;
      let largest = 0;
      let mark = performance.now();
      const ask = new WebAssembly.Suspending(async () => {
        const start = performance.now();
        gaps.push(start - mark);
        const length = exports.ti4_update_len();
        largest = Math.max(largest, length);
        let index = 0;
        if (read) {
          const { nonce, choice } = JSON.parse(text(exports.ti4_update_ptr(), length)).pending_choice;
          if (undone !== null) {
            undos[undone.at] = start - undone.start;
            gaps.pop();
            undone = null;
          } else if (undoAt.delete(Number(nonce)) && exports.ti4_can_undo()) {
            answers.pop();
            gaps.pop();
            undone = { at: Number(nonce), start: performance.now() };
            mark = undone.start;
            return -2;
          }
          index = (Number(nonce) * 7) % choice.options.length;
          answers.push(choice.options[index].id);
        }
        mark = performance.now();
        inAsk += mark - start;
        return index;
      });
      const { instance } = await WebAssembly.instantiate(bytes, { host: { ask } });
      exports = instance.exports;
      const start = performance.now();
      const status = await WebAssembly.promising(exports.ti4_play)(seed, players, humans);
      const result = text(exports.ti4_response_ptr(), Math.abs(status));
      return { gaps, answers, inAsk, largest, undos, total: performance.now() - start, status, result };
    };
    const game = await play(true);
    const sorted = [...game.gaps].sort((a, b) => a - b);
    const at = (share) => sorted[Math.min(sorted.length - 1, Math.floor(sorted.length * share))];
    // The time from the start to answer n is what a resume or an undo at n costs.
    const upTo = (n) => game.gaps.slice(0, n).reduce((sum, gap) => sum + gap, 0);

    // The save is rewritten whole after each answer.
    const write = (count) => {
      const answers = Array.from({ length: count }, (_, i) => game.answers[i % game.answers.length]);
      const save = { format: 1, engine: "0".repeat(64), seed, players, humans, answers };
      const rounds = 200;
      const start = performance.now();
      let bytes = 0;
      for (let i = 0; i < rounds; i += 1) {
        const body = JSON.stringify(save);
        localStorage.setItem("ti4.bench", body);
        bytes = body.length;
      }
      localStorage.removeItem("ti4.bench");
      return { count, bytes, ms: (performance.now() - start) / rounds };
    };
    const counts = [50, 150, game.answers.length, 5000, 50000];
    return {
      status: game.status,
      result: game.result,
      answers: game.answers.length,
      totalMs: game.total,
      readingUpdatesMs: game.inAsk,
      largestUpdate: game.largest,
      gapMs: { median: at(0.5), p90: at(0.9), p99: at(0.99), max: sorted.at(-1) },
      replayMs: Object.fromEntries(
        [50, 150, 300, game.answers.length].map((n) => [Math.min(n, game.answers.length), upTo(n)]),
      ),
      undoMs: game.undos,
      storage: counts.map(write),
    };
  },
  { base64: wasm, seed: Number(seed), players: Number(players), humans: Number(humans) },
);
await browser.close();
console.log(JSON.stringify(report, null, 2));
