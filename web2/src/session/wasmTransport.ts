// The engine in this page: `crates/ti4-wasm`, built by `scripts/build-wasm.sh`. A game runs in a
// Web Worker (`web-shared/src/engine/worker.ts`) and speaks the protocol of the server through
// the hosted exports: `ClientMessage` in, `ServerMessage` out, over `engineLink`, read by the
// code of the server's transport (`messageTransport.ts`). Step, "Play as" and the wait are local
// commands of the engine (`step`, `switch_seat`, `stepping`). The engine keeps the staged work of
// each seat in its saved game, as the server keeps it for the seat, and sends a ready draft of a
// secondary itself. The questions that need no game are exports of their own, asked in the page.
import { loadCatalog } from "@ti4/shared/content/catalog.ts";
import { type HostSave as EngineSave, Host, type StartConfig } from "@ti4/shared/engine/host.ts";
import { sha256Hex } from "@ti4/shared/engine/identity.ts";
import { hasJspi } from "@ti4/shared/engine/jspi.ts";
import { groundOdds, spaceOdds } from "@ti4/shared/engine/odds.ts";
import { inlinePort, workerPort } from "@ti4/shared/engine/port.ts";
import type { HostPort, ModelFile } from "@ti4/shared/engine/protocol.ts";
import { engineLink } from "@ti4/shared/link/engineLink.ts";
import type { GameLink, HostCommand } from "@ti4/shared/link/link.ts";
import type { HostSave as StoredSave } from "@ti4/shared/storage/index.ts";
import { type MessageCore, messageTransport } from "./messageTransport";
import type { LocalGame } from "./savedGame";
import { SLOTS, type StagedSlot, sameStaged } from "./seatStore";
import type { Capabilities, Transport } from "./transport";
import type {
  Catalog,
  ClientMessage,
  ModelEntry,
  OptionsRequest,
  ReactionModes,
  SeatStaged,
  SeatSwitch,
  TableDraw,
  TableOptions,
  TablePreview,
  TableRequest,
} from "./wire";

interface Exports {
  memory: WebAssembly.Memory;
  // The questions that need no game: the request buffer in, the response buffer out.
  ti4_setup_options(length: number): number;
  ti4_setup_preview(length: number): number;
  ti4_setup_draw(length: number): number;
  ti4_scenarios(): number;
  ti4_request_ptr(): number;
  ti4_response_ptr(): number;
}

/**
 * What the engine keeps of a game to play it again (`HostSession::save_state`, format 2, the save
 * of the save store): the game, the option id of every decision of every seat, in order
 * (`decisions`), the answers of the page among them, and the seat changes. The rest (the redo
 * list, the times of the events, the hash of the decisions) is given back as it was.
 */
export interface HostSave extends StoredSave {
  switches?: SeatSwitch[];
}

/** What a save keeps of the staged work of each seat. */
type SavedStaged = Record<string, SeatStaged>;

/** The highest revision of the staged work of any seat of a save. */
const topRevision = (staged: SavedStaged) =>
  Math.max(0, ...Object.values(staged).map((each) => each.revision));

/**
 * The save with the values of `slots` staged for `seat`, each with a new revision. A value that
 * the save has already is left as it is. Null: nothing changed.
 */
function stagedInto(
  save: HostSave,
  seat: string,
  slots: Partial<Record<StagedSlot, unknown>>,
): HostSave | null {
  const staged = { ...(save.staged as SavedStaged | undefined) };
  const kept = staged[seat] ?? { revision: 0, slots: {} };
  let revision = topRevision(staged);
  const next = { ...kept.slots };
  let changed = false;
  for (const slot of SLOTS) {
    if (!(slot in slots)) {
      continue;
    }
    const value = slots[slot] ?? null;
    if (!sameStaged(value, next[slot]?.value ?? null)) {
      changed = true;
      revision += 1;
      next[slot] = value === null ? { revision } : { revision, value };
    }
  }
  return changed ? { ...save, staged: { ...staged, [seat]: { revision, slots: next } } } : null;
}

/** Leaves the game: an instance that plays no game answers every question with it. */
const LEAVE = -1;
/** The id of every game in this page. */
const GAME_ID = "local";

/** A prepared board that the engine can start a game from (`ti4_host::view::scenario`). */
export interface ScenarioInfo {
  id: string;
  title: string;
  description: string;
  seed: number;
  players: number;
}

/** The engine file, compiled once. Each game, and each undo that plays it again, makes an instance of it in a Worker. */
export interface Engine {
  module: WebAssembly.Module;
  /** SHA-256 of the file, in hex. A saved game names the engine that played it. */
  hash: string;
  scenarios: ScenarioInfo[];
  /** The printed cards. Null for an engine that was not asked. */
  catalog: Catalog | null;
}

/**
 * Asks the engine a question that needs no game: the instance is asked nothing. With `request`
 * the question reads it from the request buffer, and is given its length.
 */
async function ask<T>(
  module: WebAssembly.Module,
  question: (exports: Exports, length: number) => number,
  request?: unknown,
) {
  const instance = await WebAssembly.instantiate(module, { host: { ask: () => LEAVE } });
  return askOn<T>(instance.exports as unknown as Exports, question, request);
}

/** Asks an instance that plays no game. */
function askOn<T>(
  exports: Exports,
  question: (exports: Exports, length: number) => number,
  request?: unknown,
) {
  const bytes = new TextEncoder().encode(request === undefined ? "" : JSON.stringify(request));
  new Uint8Array(exports.memory.buffer).set(bytes, exports.ti4_request_ptr());
  const status = question(exports, bytes.length);
  const text = new TextDecoder().decode(
    new Uint8Array(exports.memory.buffer, exports.ti4_response_ptr(), Math.abs(status)),
  );
  if (status < 0) {
    throw new Error(text);
  }
  return JSON.parse(text) as T;
}

/** Asks the engine for its scenarios. */
export const scenariosOf = (module: WebAssembly.Module) =>
  ask<ScenarioInfo[]>(module, (exports) => exports.ti4_scenarios());

/**
 * A question of the engine that needs no game (`ti4_host_query`: `describe`, `catalog`), on an
 * instance of its own. `loadCatalog({ query: engineQuery(module) })` asks it for the printed cards.
 */
export const engineQuery = (module: WebAssembly.Module) => async (request: object) =>
  (await Host.create(module)).query(request);

/**
 * What a host can choose for the table of the page, with the maps that build for `players`
 * seats (0: every map). The same options as `GET /api/setup-options` of the server; each of
 * `models` is the kind `mlp:<name>`.
 */
export const setupOptionsOf = (
  module: WebAssembly.Module,
  players: number,
  models: ModelEntry[] = [],
) =>
  ask<TableOptions>(module, (exports, length) => exports.ti4_setup_options(length), {
    players,
    models,
  } satisfies OptionsRequest);

/** The board of a table before it starts, with a place for each seat. */
export const setupPreviewOf = (module: WebAssembly.Module, table: TableRequest) =>
  ask<TablePreview>(module, (exports, length) => exports.ti4_setup_preview(length), table);

/** What the start draws for a table: who sits where, and the faction of every seat. */
export const setupDrawOf = (module: WebAssembly.Module, table: TableRequest) =>
  ask<TableDraw>(module, (exports, length) => exports.ti4_setup_draw(length), table);

/** Fetches and compiles `ti4.wasm` from `wasmUrl`. */
export async function loadEngine(wasmUrl: string): Promise<Engine> {
  if (!hasJspi()) {
    throw new Error(
      "This browser has no WebAssembly JSPI, which local play needs. Chrome 137 or later has it.",
    );
  }
  const response = await fetch(wasmUrl);
  if (!response.ok) {
    throw new Error(
      `The engine was not found (${response.status} for ${wasmUrl}). Build it with web2/scripts/build-wasm.sh.`,
    );
  }
  const bytes = await response.arrayBuffer();
  const module = await WebAssembly.compile(bytes);
  return {
    module,
    hash: await sha256Hex(bytes),
    scenarios: await scenariosOf(module),
    catalog: await loadCatalog({ query: engineQuery(module) }),
  };
}

export interface LocalPlay {
  /** The game of an earlier visit (`HostSave`). It is played again before the first update. */
  save?: HostSave;
  /**
   * Told what plays the game again, to keep it: after each update of the game and at its end.
   * The engine saves it, as the server keeps a game: no seat decides a saved decision again.
   */
  onSave?(save: HostSave): void;
  /**
   * Told when the page is left (or hidden), with the newest save: with what the seat that the page
   * is shown as staged since the save before. A write of it (`onSave`) may not end before the page
   * is gone; this is told so that the app keeps it at once.
   */
  onLeave?(save: HostSave): void;
  /** A new game is kept from its first question on, also with no decision yet (a game that a table started). */
  keepFirst?: boolean;
  /** The game waits before each decision of a seat that is not played here. */
  stepping?: boolean;
  /** The mode of each reaction that the seats of this page set, by its key. */
  modes?: Readonly<ReactionModes>;
  /** Told all modes after each change, to keep them. */
  onModes?(modes: ReactionModes): void;
  /**
   * The seats that the rule bot plays where this page does not, as `LocalGame.humans`. The
   * other seats decide at random. Not read with `LocalGame.table`, which names the kind of each
   * player.
   */
  ruleSeats?: number;
  /**
   * The wait of a table, in milliseconds: a seat that is not played here takes about this long
   * before each answer, and the page is shown the game while it waits. It does not wait to
   * decline a reaction window, as a seat on the server does not. The game is the same game with
   * every wait, and a game that is played again does not wait for its saved decisions.
   */
  waitMs?: number;
  /**
   * The files of the MLP models that the seats of the game play (`mlp:<name>`), loaded into the
   * engine in the Worker before the game starts (`models.ts`). Their buffers are transferred.
   */
  models?: ModelFile[];
}

/**
 * The commands that let the game go on and that the engine does not check against the game it
 * shows: a second one would be taken by the next wait. An answer, a plan and an undo carry the
 * version that they are for.
 */
const MOVES = new Set(["step"]);

/** A Worker that runs the engine; in a place without Workers (Node, the tests) this thread. */
function enginePort(module: WebAssembly.Module): HostPort {
  if (typeof Worker === "undefined") {
    return inlinePort(module);
  }
  // A relative path, not `@ti4/shared`: vite bundles a Worker only from a literal `new URL`.
  const worker = new Worker(new URL("../../../web-shared/src/engine/worker.ts", import.meta.url), {
    type: "module",
  });
  return workerPort(worker, module);
}

/**
 * Plays one game, in a Worker. The first update comes when the saved game is played again and
 * the game asks a seat of this page, or waits before a decision of another seat.
 */
export function createWasmTransport(
  engine: Engine,
  game: LocalGame,
  play: LocalPlay = {},
): Transport {
  let stepping = play.stepping ?? false;
  let waitMs = play.waitMs ?? 0;
  /** What the engine is told: it asks before each decision of another seat, or before each that takes the wait, or not at all. */
  const pauses = () => (stepping ? 1 : waitMs > 0 ? 2 : 0);
  /** The engine waits for the page: at a question, or before a decision of another seat. */
  let parked = false;
  /** The game is over: a command still reaches its session (an undo). */
  let ended = false;
  /** The game waits before a decision of this seat, which the page does not play. */
  let waiting: string | null = null;
  /** The wait of the seat that the game waits for. */
  let timer: ReturnType<typeof setTimeout> | null = null;
  let core: MessageCore | null = null;
  const settle = () => {
    if (timer !== null) {
      clearTimeout(timer);
      timer = null;
    }
  };

  // A save names its game: the engine plays the game of the save.
  const config: StartConfig = {
    ...(play.save
      ? {}
      : game.table
        ? { table: game.table }
        : {
            local: {
              seed: game.seed,
              players: game.players,
              humans: game.humans,
              ...(game.scenario === undefined ? {} : { scenario: game.scenario }),
              bots: Array.from({ length: game.players }, (_, seat) =>
                ((play.ruleSeats ?? 0) >> seat) & 1 ? ("smart" as const) : null,
              ),
            },
          }),
    ...(play.save ? { resume: play.save as unknown as EngineSave } : {}),
    modes: { ...play.modes },
    stepping: pauses(),
    staged: true,
  };
  const link = engineLink(enginePort(engine.module), config, {
    ...(play.models?.length ? { models: play.models } : {}),
    gate(envelope) {
      // The engine waits for the page from here on: what is sent reaches this question.
      settle();
      parked = true;
      ended = false;
      waiting = envelope.waiting ?? null;
      core?.flushModes();
    },
  });

  /** A command, and its reply. A step does not wait for its reply: no second one is sent. */
  const request = async (command: ClientMessage | HostCommand) => {
    const moves = MOVES.has(command.type);
    const was = parked;
    if (moves) {
      parked = false;
    }
    const envelope = await link.request(command);
    if (envelope.restart) {
      // The game is played again: its next question comes from a new run.
      parked = ended = false;
      waiting = null;
    } else if (envelope.resume !== undefined) {
      parked = false;
      waiting = null;
      settle();
    } else if (moves) {
      parked = was;
    }
    return envelope;
  };
  const local = (command: ClientMessage | HostCommand) => {
    request(command).catch(() => undefined);
  };
  const linked: GameLink = {
    send: local,
    request,
    subscribe: link.subscribe,
    close: () => link.close(),
  };

  /** The app was told a save: an undo back to the start is a save too. */
  let kept = play.save !== undefined || !!play.keepFirst;
  /** The save that the app was told last. */
  let last: HostSave | null = null;
  /**
   * Tells the app what plays the game again. A new game with no decision yet has nothing to
   * keep, unless the seat staged something in it (a draft at the first question of a scenario).
   */
  const keep = (save: HostSave) => {
    last = save;
    const staged = save.staged;
    const hasStaged = !!staged && typeof staged === "object" && Object.keys(staged).length > 0;
    if (save.decisions.length > 0 || kept || hasStaged) {
      kept = true;
      play.onSave?.(save);
    }
  };
  link.subscribe((event) => {
    if (event.kind === "save") {
      keep(event.save as unknown as HostSave);
    }
  });

  /** Lets the seat decide that the game waits for. */
  const step = () => {
    if (waiting !== null && parked) {
      local({ type: "step" });
    }
  };
  /** The game waits before a decision of another seat: with Step for the page, else for the wait of the table. */
  const waitFor = () => {
    settle();
    if (stepping) {
      return;
    }
    if (waitMs === 0) {
      return step();
    }
    timer = setTimeout(
      () => {
        timer = null;
        step();
      },
      waitMs * (0.6 + 0.8 * Math.random()),
    );
  };
  /** Each question and the end, after what they told: a wait starts, a save that does not fit is said. */
  let watching: { cancel(): void } | null = null;
  const watch = () => {
    const next = link.nextQuestion();
    watching = next;
    next.promise.then(
      (envelope) => {
        if (envelope?.kind === "end") {
          parked = false;
          ended = true;
          waiting = null;
          const diverged = (envelope.end as { diverged?: string | null } | undefined)?.diverged;
          if (diverged) {
            core?.fail(`The saved game does not fit this engine: ${diverged}. Start a new game.`);
            return;
          }
        } else if (waiting !== null && parked) {
          waitFor();
        }
        watch();
      },
      () => undefined,
    );
  };
  watch();

  /** Tells the engine whether it waits for each seat. Only a flag: it may be set at any time. */
  const tellStepping = () => local({ type: "stepping", mode: pauses() });
  const isPlayer = () => core?.latest()?.update.viewer.role === "player";

  // The tools of a local game. The saved game and the scenarios are of the app that made this
  // transport (`onSave`, `Engine.scenarios`).
  const capabilities: Capabilities = {
    stepping: {
      set(on) {
        stepping = on;
        tellStepping();
        if (waiting !== null && parked) {
          if (on) {
            // The seat waits for Step now, not for its time.
            settle();
            core?.again();
          } else {
            step();
          }
        }
      },
      step,
    },
    switchSeat(seat) {
      if (!parked || core?.sending() || !isPlayer()) {
        return;
      }
      local({ type: "switch_seat", seat });
    },
    wait(ms) {
      waitMs = Math.max(0, ms);
      tellStepping();
      if (!stepping) {
        step();
      }
    },
    scenarios: true,
    save: true,
  };

  core = messageTransport(linked, {
    gameId: GAME_ID,
    catalog: engine.catalog,
    // The odds are asked of the Worker: of the instance of the game, also while it waits.
    odds: (request) => spaceOdds(link.query, request),
    groundOdds: (request) => groundOdds(link.query, request),
    capabilities,
    player: true,
    ready: () => parked || ended,
    modes: { initial: play.modes, onModes: play.onModes },
    passedEach: true,
    stepping: () => (stepping ? waiting : null),
    // The save is kept after each write of the staged work.
    afterPut() {
      request({ type: "save_state" }).then(
        (envelope) => envelope.save && keep(envelope.save as unknown as HostSave),
        () => undefined,
      );
    },
    closed() {
      settle();
      watching?.cancel();
      if (typeof window !== "undefined") {
        window.removeEventListener("pagehide", stay);
        document.removeEventListener("visibilitychange", hidden);
      }
    },
  });

  /**
   * The page is left: a write of the staged work to the Worker would not come back. What the
   * seat that the page is shown as staged since the last save is put into that save here, and
   * the app is told the newest save.
   */
  const stay = () => {
    if (!last) {
      return;
    }
    const store = core?.transport.staged;
    const viewer = core?.latest()?.update.viewer;
    if (store && viewer?.role === "player") {
      const values = Object.fromEntries(SLOTS.map((slot) => [slot, store.get(slot) ?? null]));
      const left = stagedInto(last, viewer.seat, values);
      if (left) {
        keep(left);
      }
    }
    play.onLeave?.(last);
  };
  const hidden = () => document.visibilityState === "hidden" && stay();
  if (typeof window !== "undefined") {
    window.addEventListener("pagehide", stay);
    document.addEventListener("visibilitychange", hidden);
  }
  return core.transport;
}
