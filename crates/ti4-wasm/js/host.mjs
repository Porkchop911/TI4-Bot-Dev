// The hosted game in a real WebAssembly instance with JSPI: a small reference host, and a smoke
// test of the whole flow (setup form data, a game, undo, save, resume in a new instance).
//
//   node crates/ti4-wasm/js/host.mjs <ti4_wasm.wasm>
//
// `Host` is what the Web Worker of the page does (see README.md, "The hosted game"). It needs
// Node 24 or a Chromium with JSPI (Chrome and Edge 137 and later).
import { readFileSync } from "node:fs";
import assert from "node:assert/strict";

/** The answer to a `progress` output: go on. */
const CONTINUE = -5;
const LEAVE = -1;
const UNDO = -2;

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class Host {
  /** @param {WebAssembly.Module} module */
  static async create(module) {
    const host = new Host();
    const ask = new WebAssembly.Suspending(() => host.#ask());
    const { exports } = await WebAssembly.instantiate(module, { host: { ask } });
    host.exports = exports;
    host.#start = WebAssembly.promising(exports.ti4_host_start);
    return host;
  }

  #start;
  #parked = null;
  /** Called with each `ask` envelope; it answers by calling `command` (which resumes the game). */
  onAsk = () => {};
  /** Called with each `progress` envelope while a save is played again. */
  onProgress = () => {};

  #write(object) {
    const bytes = encoder.encode(JSON.stringify(object));
    const pointer = this.exports.ti4_host_in(bytes.length);
    new Uint8Array(this.exports.memory.buffer, pointer, bytes.length).set(bytes);
    return bytes.length;
  }

  /** The last output, as text. Read again after every call: the buffer can move. */
  #out() {
    const pointer = this.exports.ti4_host_out_ptr();
    const length = this.exports.ti4_host_out_len();
    return decoder.decode(new Uint8Array(this.exports.memory.buffer, pointer, length));
  }

  #result(status) {
    const text = this.#out();
    if (status < 0) throw new Error(text);
    return JSON.parse(text);
  }

  #time() {
    this.exports.ti4_host_set_time(Date.now(), performance.now());
  }

  /** Stateless questions of the setup form: `describe`, `map_preview`, `catalog`. */
  query(request) {
    return this.#result(this.exports.ti4_host_query(this.#write(request)));
  }

  /**
   * A command (a `ClientMessage`, or `history`, `save_state`, `leave`, `submit_batch`). Returns
   * the `command` envelope. When it has `resume`, the parked game goes on with that code.
   */
  command(request) {
    const envelope = this.#result(this.exports.ti4_host_command(this.#write(request)));
    if (envelope.resume !== undefined && this.#parked) {
      const { resolve } = this.#parked;
      this.#parked = null;
      this.#time();
      resolve(envelope.resume);
    }
    return envelope;
  }

  /** Play a game; resolves with the `end` envelope when it ends or is left. */
  async start(config) {
    this.#time();
    const status = await this.#start(this.#write(config));
    return this.#result(status);
  }

  // The import: the game asks the page. Parks this (JSPI) until `command` resumes it.
  #ask() {
    const envelope = JSON.parse(this.#out());
    if (envelope.kind === "progress") {
      this.onProgress(envelope.progress);
      return Promise.resolve(CONTINUE);
    }
    return new Promise((resolve) => {
      this.#parked = { resolve };
      // The page runs here (the stack of the game is parked): it may send commands.
      queueMicrotask(() => this.onAsk(envelope));
    });
  }
}

// --- the smoke test ------------------------------------------------------------------------

const GAME = "local";
const message = (host, type, fields) =>
  host.command({ type, protocol_version: 4, game_id: GAME, ...fields });

/** The person: the first option, every time. Collects what the page received. */
function person(host, log, limit) {
  host.onAsk = (envelope) => {
    log.messages.push(...envelope.messages);
    const question = [...log.messages]
      .reverse()
      .find((m) => (m.type === "state_update" || m.type === "initial_snapshot") && m.pending_choice);
    log.question = question;
    if (log.answers.length >= limit) {
      const save = host.command({ type: "save_state" }).save;
      log.save = save;
      host.command({ type: "leave" });
      return;
    }
    if (log.undoAt === log.answers.length && !log.undone) {
      log.undone = true;
      const reply = message(host, "undo", { expected_version: question.game_version });
      assert.equal(reply.resume, UNDO, JSON.stringify(reply));
      return;
    }
    const choice = question.pending_choice.choice;
    const option = choice.options[0].id;
    const reply = message(host, "submit_choice", {
      nonce: question.pending_choice.nonce,
      expected_version: question.game_version,
      option_id: option,
    });
    log.messages.push(...reply.messages);
    assert.ok(reply.resume !== undefined, JSON.stringify(reply));
    log.answers.push(option);
  };
}

async function smoke(wasmPath) {
  const module = await WebAssembly.compile(readFileSync(wasmPath));
  const setup = { game_id: GAME, seed: 5, players: 4, human_seat: 1, human_faction: "mentak" };

  // 1. The setup form.
  const form = (await Host.create(module)).query({ query: "describe" });
  assert.deepEqual(Object.keys(form.maps), ["2", "3", "4", "5", "6", "7", "8"]);
  assert.equal(form.strategy_card_sets.length, 3);
  const picture = (await Host.create(module)).query({
    query: "map_preview", player_count: 4, choice: { kind: "random" },
  });
  assert.equal(picture.seats.length, 4);

  // 2. A game with an undo in it; leave after 40 answers with a save.
  const first = { messages: [], answers: [], undoAt: 12 };
  const host = await Host.create(module);
  person(host, first, 40);
  const started = performance.now();
  const end = await host.start({ setup });
  assert.equal(end.end.left, true, JSON.stringify(end));
  assert.equal(first.messages[0].type, "initial_snapshot");
  const snapshots = first.messages.filter((m) => m.type === "initial_snapshot");
  assert.equal(snapshots.length, 2, "a snapshot at the start and one after the undo");
  assert.equal(snapshots[1].history.generation, 1);
  assert.equal(first.messages.filter((m) => m.type === "action_rejected").length, 0);
  const played = performance.now() - started;

  // 3. The save plays again in a new instance and comes back to the same question.
  const save = first.save;
  // One of the 40 answers was taken back by the undo.
  assert.equal(save.answers.length, first.answers.length - 1);
  const second = { messages: [], answers: [] };
  const again = await Host.create(module);
  const progress = [];
  again.onProgress = (p) => progress.push(p);
  again.onAsk = (envelope) => {
    second.messages.push(...envelope.messages);
    second.check = envelope.resume_check;
    again.command({ type: "leave" });
  };
  const resumed = performance.now();
  // The save names its game: a resume needs nothing else.
  assert.equal(save.format, 2);
  await again.start({ resume: save });
  const replayed = performance.now() - resumed;
  assert.equal(second.check.ok, true, JSON.stringify(second.check));
  // The save has every decision of the game (`decisions`): each is given again, and no bot decides.
  assert.ok(progress.length >= 1 && progress.at(-1).total === save.decisions.length, JSON.stringify(progress));
  assert.deepEqual(second.messages[0].state, first.question.state);
  assert.deepEqual(second.messages[0].pending_choice, first.question.pending_choice);
  const lastSnapshot = first.messages.map((m) => m.type).lastIndexOf("initial_snapshot");
  const firstEvents = first.messages[lastSnapshot].events.concat(
    first.messages.slice(lastSnapshot).filter((m) => m.type === "event").map((m) => m.entry),
  );
  assert.deepEqual(second.messages[0].events, firstEvents, "the event log, times included");

  console.log(JSON.stringify({
    ok: true,
    wasm_bytes: readFileSync(wasmPath).length,
    form_maps: Object.fromEntries(Object.entries(form.maps).map(([n, m]) => [n, m.length])),
    answers: first.answers.length,
    play_ms: Math.round(played),
    replay_ms: Math.round(replayed),
    progress_reports: progress.length,
    json_bytes_of_first_snapshot: JSON.stringify(first.messages[0]).length,
  }));
}

/** `--long`: one person who always takes the first option, to the end or 3000 answers. */
async function long(wasmPath) {
  const module = await WebAssembly.compile(readFileSync(wasmPath));
  const host = await Host.create(module);
  const log = { messages: [], answers: [] };
  person(host, log, 3000);
  const started = performance.now();
  const end = await host.start({ setup: { game_id: GAME, seed: 9, players: 6, human_seat: 0 } });
  console.log(JSON.stringify({
    end: end.end,
    answers: log.answers.length,
    ms: Math.round(performance.now() - started),
    messages: log.messages.length,
    wasm_memory_mb: host.exports.memory.buffer.byteLength / 1048576,
  }));
}

/**
 * `--record <out.json>`: the messages of a game of four, with an undo and a rewind in it, as a
 * client receives them in order (a fixture for the contract tests of the page).
 */
async function record(wasmPath, outPath) {
  const { writeFileSync } = await import("node:fs");
  const module = await WebAssembly.compile(readFileSync(wasmPath));
  const log = { messages: [], answers: [], undoAt: 10 };
  const host = await Host.create(module);
  person(host, log, 120);
  const end = await host.start({ setup: { game_id: GAME, seed: 17, players: 4, human_seat: 0 } });
  log.messages.push(...end.messages);
  writeFileSync(outPath, JSON.stringify({ game_id: GAME, messages: log.messages }));
  console.log(`${log.messages.length} messages, ${log.answers.length} answers -> ${outPath}`);
}

if (process.argv[1].endsWith("host.mjs") && process.argv[2]) {
  const [, , wasm, mode, out] = process.argv;
  await (mode === "--long" ? long(wasm) : mode === "--record" ? record(wasm, out) : smoke(wasm));
}
