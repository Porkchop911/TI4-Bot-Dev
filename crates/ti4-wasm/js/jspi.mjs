// JSPI proof: the engine's blocking decider is answered from a promise in Chromium.
// Usage (from web2/, which has Playwright): node ../crates/ti4-wasm/js/jspi.mjs <file.wasm>
import { readFileSync } from "node:fs";
import { createRequire } from "node:module";

const require = createRequire(`${process.cwd()}/`);
const { chromium } = require("@playwright/test");

const wasm = readFileSync(process.argv[2]).toString("base64");
const browser = await chromium.launch();
const page = await browser.newPage();
const report = await page.evaluate(async (base64) => {
  if (!("Suspending" in WebAssembly)) return { error: "no JSPI in this browser" };
  const bytes = Uint8Array.from(atob(base64), (c) => c.charCodeAt(0));
  let exports;
  const text = (pointer, length) =>
    new TextDecoder().decode(new Uint8Array(exports.memory.buffer, pointer, length));
  // An export returns a length, negated when the response buffer holds an error text.
  const response = (status) => {
    const body = text(exports.ti4_response_ptr(), Math.abs(status));
    if (status < 0) throw new Error(body);
    return JSON.parse(body);
  };
  const asked = [];
  const views = [];
  // The engine calls this synchronously; JSPI parks the wasm stack until the promise settles.
  const ask = new WebAssembly.Suspending(async () => {
    // A real timer tick: the event loop runs while the game is parked.
    await new Promise((resolve) => setTimeout(resolve, 5));
    // A negative answer leaves the game: the engine unwinds and the export returns.
    if (asked.length === 10) return -1;
    // A query into the same instance while its game is suspended: the update for the host.
    const update = JSON.parse(text(exports.ti4_update_ptr(), exports.ti4_update_len()));
    const { nonce, choice } = update.pending_choice;
    asked.push(`${nonce} ${choice.player}: ${choice.prompt} [${choice.options.length}]`);
    // The view of the asked seat: its own hand is listed, another seat's is only a count. Nobody
    // holds a card this early (`otherCards` is 0), so this shows the shape, not the redaction.
    const view = update.view;
    // An empty list is left out of the JSON.
    const held = (player) => player.held_action_cards ?? [];
    const own = view.players.find((player) => player.id === choice.player);
    const others = view.players.filter((player) => player.id !== choice.player);
    views.push({
      bytes: exports.ti4_update_len(),
      viewerIsAsked: update.viewer.seat === choice.player,
      players: view.players.length,
      tiles: (view.board.map_tiles ?? []).length,
      ownHandListed: held(own).length === own.action_cards_count,
      otherHandsHidden: others.every((player) => held(player).length === 0),
      otherCards: others.reduce((sum, player) => sum + player.action_cards_count, 0),
    });
    return choice.options.length - 1;
  });
  ({ exports } = (await WebAssembly.instantiate(bytes, { host: { ask } })).instance);
  const play = WebAssembly.promising(exports.ti4_play);
  const started = performance.now();
  // Seed 7, eight seats, seat a on the host.
  const running = play(7, 8, 1);
  const pendingBeforeFirstAnswer = running instanceof Promise;
  const result = response(await running);
  return { pendingBeforeFirstAnswer, asked, views, result, ms: Math.round(performance.now() - started) };
}, wasm);
console.log(`chromium ${browser.version()}`);
console.log(JSON.stringify(report, null, 2));
await browser.close();
const viewsHold = report.views?.every(
  (view) =>
    view.players === 8 &&
    view.tiles > 0 &&
    view.viewerIsAsked &&
    view.ownHandListed &&
    view.otherHandsHidden,
);
if (report.error || report.asked.length !== 10 || !viewsHold) process.exit(1);
