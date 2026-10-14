// The engine in this page: `crates/ti4-wasm`, built by `scripts/build-wasm.sh`.
import type { LocalGame, SeatSwitch } from "./savedGame";
import { stepOf } from "./movementPlan";
import type { Capabilities, PlanStopped, Transport, TransportEvent } from "./transport";
import type {
  Catalog,
  ChoiceOption,
  DraftOutcome,
  DraftScope,
  DraftStep,
  ReactionMode,
  ReactionModes,
  SessionUpdate,
  SpaceOddsRequest,
  SpaceOddsResponse,
  TableDraw,
  TableOptions,
  TablePreview,
  TableRequest,
} from "./wire";

interface Exports {
  memory: WebAssembly.Memory;
  ti4_play(seed: number, players: number, humans: number): number;
  ti4_play_scenario(humans: number, idLength: number): number;
  ti4_play_setup(length: number): number;
  ti4_setup_options(players: number): number;
  ti4_setup_preview(length: number): number;
  ti4_setup_draw(length: number): number;
  ti4_set_rule_seats(mask: number): void;
  ti4_scenarios(): number;
  ti4_catalog(): number;
  ti4_space_odds(length: number): number;
  ti4_can_undo(): number;
  ti4_response_ptr(): number;
  ti4_update_ptr(): number;
  ti4_update_len(): number;
  /** 0: the other seats play on. 1: the engine asks before each of their decisions. 2: before each that takes the wait of a table. */
  ti4_set_stepping(mode: number): void;
  ti4_decided(): number;
  ti4_switch_seats(decision: number, humans: number): void;
  ti4_request_ptr(): number;
  ti4_draft(length: number): number;
  ti4_draft_ptr(): number;
  ti4_plan(length: number): number;
  ti4_set_reaction_mode(length: number): number;
  ti4_journal(): number;
  ti4_replay(decisions: number): void;
  ti4_saved_option(length: number): number;
}

/**
 * What a script answers to the choice that the engine is parked in (`ti4_plan`,
 * `ti4_view::planning::draft::script_answer`).
 */
type PlanAnswer =
  /**
   * Answer the choice with the option at `index`. `next` is the first entry that is still to
   * play. With `script` the script goes on in this order: a payment changed its place.
   */
  | { kind: "answer"; index: number; next: number; script?: DraftStep[] }
  /** Every entry is played: the choice is not of the script. */
  | { kind: "done" }
  /** The game asks something else first (a reaction window, another seat). */
  | { kind: "interrupted"; reason: string }
  /** The game does not offer what the entry asks for. */
  | { kind: "mismatch"; reason: string };

/** A window that the engine declined for this side. */
interface Passed {
  option: ChoiceOption;
  cards: string[];
}

/**
 * The decisions that were made without asking the page since it last read them (`ti4_journal`):
 * of the seats that are not played here, and the windows that the engine declined for this
 * side. `from` is the number of the first of them.
 */
interface Journal {
  from: number;
  entries: { id: string; passed?: Passed }[];
}

/** An option of the decision that the engine is parked in (`ti4_saved_option`). */
interface SavedOption {
  index: number;
  /** A seat of this page decides it. */
  own: boolean;
  option: ChoiceOption;
}

// JSPI: the engine asks for a decision with a blocking call. `Suspending` parks its stack until
// the promise of the answer settles, and `promising` makes the export that runs the game async.
interface Jspi {
  Suspending: new (ask: () => Promise<number>) => WebAssembly.ImportValue;
  promising<Args extends number[]>(
    run: (...args: Args) => number,
  ): (...args: Args) => Promise<number>;
}

/** Leaves the game: the engine unwinds and `ti4_play` returns. */
const LEAVE = -1;
/** Takes back the answer before the pending choice, when `ti4_can_undo` says the engine can. */
const UNDO = -2;
/** Lets a seat decide that the engine waits for while it is stepped. */
const GO = -3;
/** After `ti4_switch_seats`: the engine asks the decision again, of the new decider of its seat. */
const SWITCH = -4;

/** A replay gives the page a turn this often, so it can show how far the replay is. */
const REPLAY_SLICE_MS = 50;

/** A prepared board that the engine can start a game from (`ti4_view::scenario`). */
export interface ScenarioInfo {
  id: string;
  title: string;
  description: string;
  seed: number;
  players: number;
}

/** The engine file, compiled once. Each game and each undo makes an instance of it. */
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

/** The instance of a module that answers the questions that come often. It plays no game. */
const kept = new WeakMap<WebAssembly.Module, Promise<Exports>>();

function keptOf(module: WebAssembly.Module) {
  let exports = kept.get(module);
  if (!exports) {
    exports = WebAssembly.instantiate(module, { host: { ask: () => LEAVE } }).then(
      (instance) => instance.exports as unknown as Exports,
    );
    kept.set(module, exports);
  }
  return exports;
}

/**
 * The odds of a space combat (`ti4_space_odds`). It is asked for each change of a staged
 * movement, so one instance of the module answers all of them.
 */
export async function spaceOddsOf(module: WebAssembly.Module, request: SpaceOddsRequest) {
  const exports = await keptOf(module);
  try {
    return askOn<SpaceOddsResponse>(
      exports,
      (on, length) => on.ti4_space_odds(length),
      request,
    );
  } catch (error) {
    // A trap may leave the instance broken: the next question makes a new one.
    if (error instanceof WebAssembly.RuntimeError) {
      kept.delete(module);
    }
    throw error;
  }
}

/** Asks the engine for its scenarios. */
export const scenariosOf = (module: WebAssembly.Module) =>
  ask<ScenarioInfo[]>(module, (exports) => exports.ti4_scenarios());

/** Asks the engine for the printed cards. */
export const catalogOf = (module: WebAssembly.Module) =>
  ask<Catalog>(module, (exports) => exports.ti4_catalog());

/**
 * What a host can choose for the table of the page, with the maps that build for `players`
 * seats (0: every map). The same options as `GET /api/setup-options` of the server.
 */
export const setupOptionsOf = (module: WebAssembly.Module, players: number) =>
  ask<TableOptions>(module, (exports) => exports.ti4_setup_options(players));

/** The board of a table before it starts, with a place for each seat. */
export const setupPreviewOf = (module: WebAssembly.Module, table: TableRequest) =>
  ask<TablePreview>(module, (exports, length) => exports.ti4_setup_preview(length), table);

/** What the start draws for a table: who sits where, and the faction of every seat. */
export const setupDrawOf = (module: WebAssembly.Module, table: TableRequest) =>
  ask<TableDraw>(module, (exports, length) => exports.ti4_setup_draw(length), table);

/** Fetches and compiles `ti4.wasm` from `wasmUrl`. */
export async function loadEngine(wasmUrl: string): Promise<Engine> {
  const jspi = WebAssembly as unknown as Partial<Jspi>;
  if (!jspi.Suspending || !jspi.promising) {
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
  const digest = new Uint8Array(await crypto.subtle.digest("SHA-256", bytes));
  const module = await WebAssembly.compile(bytes);
  return {
    module,
    hash: Array.from(digest, (byte) => byte.toString(16).padStart(2, "0")).join(""),
    scenarios: await scenariosOf(module),
    catalog: await catalogOf(module),
  };
}

export interface LocalPlay {
  /**
   * The decisions of an earlier visit, of every seat (`SavedGame.decisions`). The game is played
   * again with them before the first update.
   */
  decisions?: readonly string[];
  /** The seat changes of an earlier visit. The game is played again with them. */
  switches?: readonly SeatSwitch[];
  /**
   * Told the whole lists after each decision that the page learns of, each undo and each seat
   * change, to keep them. `own` is not kept: the places in `decisions` of the answers of this
   * page, which are what an undo takes back.
   */
  onDecisions?(
    decisions: readonly string[],
    switches: readonly SeatSwitch[],
    own: readonly number[],
  ): void;
  /** The game waits before each decision of a seat that is not played here. */
  stepping?: boolean;
  /** The mode of each reaction that the seats of this page set, by its key. */
  modes?: Readonly<ReactionModes>;
  /** Told all modes after each change, to keep them. */
  onModes?(modes: ReactionModes): void;
  /**
   * The seats that the rule bot plays where this page does not, as `LocalGame.humans`
   * (`ti4_set_rule_seats`). The other seats decide at random. A game that is played again
   * needs the same mask. Not read with `table`, which names the kind of each player.
   */
  ruleSeats?: number;
  /**
   * The table that the game is started from (`ti4_play_setup`), in place of the seed, the
   * seats and the mask of the game: its seats and the factions that nobody chose are drawn
   * from its seed, as the server does it for the table of a lobby. A game that is played again
   * needs the same table.
   */
  table?: TableRequest;
  /**
   * The wait of a table, in milliseconds: a seat that is not played here takes about this long
   * before each answer, and the page is shown the game while it waits. It does not wait to
   * decline a reaction window, as a seat on the server does not. The game is the same game with
   * every wait, and a game that is played again does not wait for its saved decisions.
   */
  waitMs?: number;
}

/** A turn of the event loop. A timer would be slowed down in a tab that is not shown. */
const breathe = () =>
  new Promise<void>((resume) => {
    const channel = new MessageChannel();
    channel.port1.onmessage = () => resume();
    channel.port2.postMessage(null);
  });

/**
 * Plays one game. The first update comes when `play.decisions` are used up and the game asks a
 * seat of this page, or waits before a decision of another seat.
 */
export function createWasmTransport(
  engine: Engine,
  game: LocalGame,
  play: LocalPlay = {},
): Transport {
  const listeners = new Set<(event: TransportEvent) => void>();
  /**
   * Every decision of the game in order, of every seat: the id of the chosen option. It is the
   * saved game, as the decision log is on the server. `log[n]` answers the decision with the
   * number `n` (`ti4_decided`).
   */
  const log = [...(play.decisions ?? [])];
  /**
   * The places in `log` of the answers of this side: what an undo can take back. A game that is
   * played again fills it again.
   */
  let own: number[] = [];
  const switches = [...(play.switches ?? [])];
  /** How many answers are of seats that were played here before the last seat change: they stay. */
  const kept = () => {
    const from = switches.at(-1)?.decision ?? 0;
    return own.filter((at) => at < from).length;
  };
  /**
   * What each answer of this side chose, by its place in `own`: an id alone does not say that.
   * Compare `RecordedDecision` of the server. A game that is played again fills it again.
   */
  let recorded: ChoiceOption[] = [];
  /** Tells the app the game as it is to keep. */
  const keep = () => play.onDecisions?.(log, switches, own);
  /** An answer of this side to the decision that the game waits in. */
  const give = (option: ChoiceOption) => {
    own.push(log.length);
    log.push(option.id);
    recorded.push(option);
  };
  let latest: TransportEvent | null = null;
  /** The pending choice: its update, its option ids in order, and how to answer the engine. */
  let pending: {
    nonce: string;
    update: SessionUpdate;
    options: string[];
    answer: (index: number) => void;
    /** The engine has a checkpoint for the answer before this choice. */
    canGoBack: boolean;
    /** The game waits before a decision of another seat: there is no choice to answer. */
    paused: boolean;
    /** Runs a draft script on the instance that waits here. */
    draft: (script: DraftStep[], scope?: DraftScope) => DraftOutcome;
    /** Tells the instance that waits here the seats of the page. Returns the number of the decision. */
    switchTo: (humans: number) => number;
  } | null = null;
  let stepping = play.stepping ?? false;
  let waitMs = play.waitMs ?? 0;
  /** What the engine is told (`ti4_set_stepping`): it asks before each decision of another seat, or before each that takes the wait, or not at all. */
  const pauses = () => (stepping ? 1 : waitMs > 0 ? 2 : 0);
  /** Tells the engine of the run that is the game whether it waits for each seat. */
  let tellStepping: () => void = () => {};
  /** The modes that the seats of this page set. The engine of each run is told. */
  const modes: ReactionModes = { ...play.modes };
  /** Tells the engine of the run that is the game the mode of a reaction. */
  let tellMode: (key: string, mode: ReactionMode) => void = () => {};
  /** Asks the engine of the run that is the game what a script answers to the choice that it waits in. */
  let askPlan: (script: DraftStep[], next: number, actor: string) => PlanAnswer = () => ({
    kind: "done",
  });
  /**
   * The answers that the engine gave itself in this run: the decline of a window of cards that
   * are never offered. By their place in `own`. An undo goes back over them.
   */
  let passed = new Set<number>();
  /** The plan that is being sent: the choices of the game are answered from its script. */
  let plan: {
    destination: string;
    /** How many entries of the script answer the choices before the steps of the plan. */
    lead: number;
    /** The answers by id, then the steps. A payment may change its place (`ti4_plan`). */
    script: DraftStep[];
    next: number;
    actor: string;
    /** How many answers the game had before the plan. */
    start: number;
    /** The game refused a step: the answers of the plan are taken back, down to `start`. */
    refused: string | null;
  } | null = null;
  /** Told with the next update: where the last plan stopped. */
  let planStopped: PlanStopped | null = null;
  /** An undo goes on while the choice that is open again is in the middle of a movement. */
  let undoing = false;
  /** What the undo took back so far, first answer first. Told with the next update. */
  let undone: ChoiceOption[] = [];
  /** The answers of each plan of this visit, as a range of `own`: an undo takes them back whole. */
  let groups: { start: number; end: number }[] = [];
  /** The number of the run that is the game. An undo starts a new run; the old one leaves. */
  let current = 0;

  /** Tells an event that is no state of the game: a listener that comes later does not get it. */
  const say = (event: TransportEvent) => {
    for (const listener of listeners) {
      listener(event);
    }
  };
  const tell = (event: TransportEvent) => {
    latest = event;
    say(event);
  };
  /** Where the answers end that this side gave itself: an undo takes the last of them back. */
  const lastOwn = () => {
    let end = own.length;
    while (end > kept() && passed.has(end - 1)) {
      end -= 1;
    }
    return end;
  };
  const canUndo = () => lastOwn() > kept();

  /** Shows the player a choice, with what the plan or the undo before it came to. */
  const show = (update: SessionUpdate, seat?: string) => {
    const steps = undone.flatMap((answer) => stepOf(answer) ?? []);
    tell({
      kind: "update",
      update,
      canUndo: canUndo(),
      plan: planStopped ?? undefined,
      undone: steps.length ? steps : undefined,
      stepping: seat === undefined ? undefined : { seat },
    });
    planStopped = null;
    undone = [];
  };

  const run = async (id: number) => {
    const jspi = WebAssembly as unknown as Jspi;
    /** How many decisions the run is given from the saved game, and how many it was given. */
    const saved = log.length;
    let replayed = 0;
    /** The app was told which of the saved decisions are answers of this side. */
    let told = saved === 0;
    own = [];
    recorded = [];
    // The engine of this run is given every saved decision, so it knows none as its own.
    passed = new Set();
    let shown = performance.now();
    let diverged: string | null = null;
    let exports!: Exports;
    const text = (pointer: number, length: number) =>
      new TextDecoder().decode(new Uint8Array(exports.memory.buffer, pointer, length));
    const readUpdate = (): SessionUpdate | null => {
      const length = exports.ti4_update_len();
      return length ? JSON.parse(text(exports.ti4_update_ptr(), length)) : null;
    };
    /** Calls an export that reads the request buffer. The result is in the response buffer, or at `result`. */
    const request = <T>(
      call: (length: number) => number,
      body: unknown,
      result: () => number = exports.ti4_response_ptr,
    ): T | null => {
      const bytes = new TextEncoder().encode(JSON.stringify(body));
      new Uint8Array(exports.memory.buffer).set(bytes, exports.ti4_request_ptr());
      const status = call(bytes.length);
      if (status < 0) {
        throw new Error(text(exports.ti4_response_ptr(), -status));
      }
      return status ? JSON.parse(text(result(), status)) : null;
    };
    const draft = (steps: DraftStep[], scope?: DraftScope) =>
      request<DraftOutcome>(exports.ti4_draft, { script: steps, scope }, exports.ti4_draft_ptr)!;
    const setMode = (key: string, mode: ReactionMode) =>
      void request(exports.ti4_set_reaction_mode, { card: key, mode });
    /** The option of a saved decision in the decision that the engine is parked in, or why it is not offered. */
    const savedOption = (wanted: string): SavedOption | string => {
      const bytes = new TextEncoder().encode(wanted);
      new Uint8Array(exports.memory.buffer).set(bytes, exports.ti4_request_ptr());
      const status = exports.ti4_saved_option(bytes.length);
      const result = text(exports.ti4_response_ptr(), Math.abs(status));
      return status < 0 ? result : JSON.parse(result);
    };
    /**
     * Brings the log up to the game: the decisions that were made since the engine last asked.
     * Returns the windows that the engine declined for this side. Each is an answer of this
     * side: an undo goes back over it.
     */
    const takeJournal = (): Passed[] => {
      const status = exports.ti4_journal();
      if (status < 0) {
        throw new Error(text(exports.ti4_response_ptr(), -status));
      }
      const { from, entries }: Journal = JSON.parse(text(exports.ti4_response_ptr(), status));
      const windows: Passed[] = [];
      if (entries.length) {
        // After an undo from a checkpoint the engine made the decisions since then again.
        log.length = Math.min(log.length, from);
        for (const { id, passed: window } of entries) {
          if (window) {
            passed.add(own.length);
            give(window.option);
            windows.push(window);
          } else {
            log.push(id);
          }
        }
        keep();
      } else if (!told) {
        keep();
      }
      told = true;
      if (log.length !== exports.ti4_decided()) {
        throw new Error(
          `The page has ${log.length} decisions of the game, and the engine made ${exports.ti4_decided()}.`,
        );
      }
      return windows;
    };
    const switchTo = (humans: number) => {
      const decision = exports.ti4_decided();
      exports.ti4_switch_seats(decision, humans);
      return decision;
    };
    const ask = new jspi.Suspending(async () => {
      if (id !== current) {
        return LEAVE;
      }
      const update = readUpdate();
      if (!update) {
        // A decision of the saved game: the engine is given its answer, whoever the seat is.
        const wanted = log[replayed];
        const found = replayed < saved ? savedOption(wanted) : "the saved game has no more";
        if (typeof found === "string") {
          diverged =
            `The saved game does not fit this engine: decision ${replayed + 1} of ${saved} ` +
            `was "${wanted}", but ${found}. Start a new game.`;
          return LEAVE;
        }
        if (found.own) {
          own.push(replayed);
          recorded.push(found.option);
        }
        replayed += 1;
        if (performance.now() - shown > REPLAY_SLICE_MS) {
          tell({ kind: "replaying", done: replayed, total: saved });
          await breathe();
          shown = performance.now();
        }
        return found.index;
      }
      // An undo that opens such a window again goes on to the answer of the player before it.
      const windows = takeJournal();
      const goesBack = windows.length > 0 && undoing && canUndo();
      for (const { cards } of goesBack ? [] : windows) {
        say({ kind: "passed", cards });
      }
      if (!update.pending_choice) {
        // The engine waits before a decision of a seat that is not played here.
        const turn = update.turn_status;
        const seat = turn.kind === "waiting_for_decision" ? turn.seat : "";
        const canGoBack = exports.ti4_can_undo() === 1;
        // A plan ends where another seat decides.
        const index = decidePlan(update, canGoBack);
        if (index !== null) {
          return index;
        }
        if (goesBack) {
          return back(canGoBack);
        }
        // An undo goes on to the choice that it opens again.
        if (undoing || pauses() === 0) {
          return GO;
        }
        return new Promise<number>((answer) => {
          const waits = {
            nonce: "",
            update,
            options: [],
            answer,
            canGoBack,
            paused: true,
            draft,
            switchTo,
          };
          pending = waits;
          if (stepping) {
            return show(update, seat);
          }
          // The wait of the table: the game is shown, and the seat decides by itself. An undo
          // or a seat change in the wait answers first, and the timer then finds another state.
          show(update);
          setTimeout(() => pending === waits && step(), waitMs * (0.6 + 0.8 * Math.random()));
        });
      }
      const { nonce, choice } = update.pending_choice;
      const options = choice.options.map((option) => option.id);
      const canGoBack = exports.ti4_can_undo() === 1;
      const index = decide(update, canGoBack, goesBack);
      if (index !== null) {
        return index;
      }
      return new Promise<number>((answer) => {
        pending = { nonce, update, options, answer, canGoBack, paused: false, draft, switchTo };
        show(update);
      });
    });
    const instance = await WebAssembly.instantiate(engine.module, { host: { ask } });
    exports = instance.exports as unknown as Exports;
    exports.ti4_set_stepping(pauses());
    tellStepping = () => id === current && exports.ti4_set_stepping(pauses());
    for (const [key, mode] of Object.entries(modes)) {
      setMode(key, mode);
    }
    tellMode = (card, mode) => id === current && setMode(card, mode);
    askPlan = (entries, next, actor) =>
      request<PlanAnswer>(exports.ti4_plan, { script: entries, next, actor })!;
    // The engine is given the saved decisions: a window that was declined then is one of them.
    exports.ti4_replay(saved);
    for (const item of switches) {
      exports.ti4_switch_seats(item.decision, item.humans);
    }
    // A status of 0 or more is the length of a result; below 0, the negated length of an error.
    let status: number;
    exports.ti4_set_rule_seats(play.table ? 0 : (play.ruleSeats ?? 0));
    if (play.table) {
      // The engine reads the table before it plays: a draft writes into the same buffer.
      const table = new TextEncoder().encode(JSON.stringify(play.table));
      new Uint8Array(exports.memory.buffer).set(table, exports.ti4_request_ptr());
      status = await jspi.promising(exports.ti4_play_setup)(table.length);
    } else if (game.scenario === undefined) {
      status = await jspi.promising(exports.ti4_play)(game.seed, game.players, game.humans);
    } else {
      // The engine copies the id before it plays: a draft writes into the same buffer.
      const name = new TextEncoder().encode(game.scenario);
      new Uint8Array(exports.memory.buffer).set(name, exports.ti4_request_ptr());
      status = await jspi.promising(exports.ti4_play_scenario)(game.humans, name.length);
    }
    if (id !== current) {
      return;
    }
    if (diverged) {
      throw new Error(diverged);
    }
    const result = text(exports.ti4_response_ptr(), Math.abs(status));
    if (status < 0) {
      throw new Error(result);
    }
    const last = readUpdate();
    for (const { cards } of takeJournal()) {
      say({ kind: "passed", cards });
    }
    if (last) {
      tell({ kind: "update", update: last, canUndo: canUndo() });
    }
    const stopped: string | null = JSON.parse(result).stopped;
    if (stopped) {
      throw new Error(stopped);
    }
  };

  const start = () => {
    const id = current;
    if (log.length > 0) {
      tell({ kind: "replaying", done: 0, total: log.length });
    }
    run(id).catch((error: unknown) => {
      if (id === current) {
        pending = null;
        tell({ kind: "error", message: error instanceof Error ? error.message : String(error) });
      }
    });
  };
  /**
   * Takes back the answer before the open choice: the engine asks that choice again, from a
   * checkpoint when it has one, or else the game is played again without the answer.
   */
  const back = (canGoBack: boolean): number => {
    /**
     * The answers from `length` on leave, with every decision of the game after the first of
     * them, and an undo keeps what they chose.
     */
    const drop = (length: number) => {
      log.length = own[length];
      own.length = length;
      passed = new Set([...passed].filter((at) => at < length));
      const gone = recorded.splice(length);
      if (undoing) {
        undone = [...gone, ...undone];
      }
      keep();
    };
    // A window that the engine declined goes with the answer before it: the engine does the same.
    const end = lastOwn();
    if (canGoBack) {
      drop(end - 1);
      return UNDO;
    }
    // A game that is played again costs the same for one answer and for many: the answers of a
    // plan go at once.
    const group = groups.findLast((item) => item.start < end && end <= item.end);
    drop(group ? group.start : end - 1);
    groups = groups.filter((item) => item.start < own.length);
    current += 1;
    start();
    return LEAVE;
  };
  /**
   * A choice in the middle of a movement (a ship has moved, or a hold is open) or of the
   * landings (the answer before it landed a unit).
   */
  const insideMovement = (update: SessionUpdate) => {
    const subtype = update.pending_choice?.choice.context?.subtype;
    return (
      subtype === "load_cargo" ||
      (subtype === "movement_step" &&
        update.tactical?.kind === "movement" &&
        update.tactical.moved > 0) ||
      (subtype === "commit_ground_forces" && recorded.at(-1)?.kind === "commit") ||
      ((subtype === "produce_unit" || subtype === "pay_resources" || subtype === "place_unit") &&
        afterPurchase())
    );
  };
  /** The answers before the open choice are of a production: a purchase, then what paid and placed it. */
  const afterPurchase = () => {
    for (let at = recorded.length - 1; at >= 0; at--) {
      const { kind } = recorded[at];
      if (kind === "produce") {
        return true;
      }
      if (kind !== "pay" && kind !== "place") {
        return false;
      }
    }
    return false;
  };
  /**
   * The answer of the plan that is being sent to the choice that the game waits in. Null when
   * there is no plan, or when it ends here: what it came to is then told with the next update.
   */
  const decidePlan = (update: SessionUpdate, canGoBack: boolean): number | null => {
    if (plan) {
      const sent = plan;
      const result: PlanAnswer = sent.refused
        ? { kind: "mismatch", reason: sent.refused }
        : askPlan(sent.script, sent.next, sent.actor);
      // The steps of the plan that are in the game, without the answers before them.
      const done = Math.max(0, sent.next - sent.lead);
      if (result.kind === "answer") {
        const option = update.pending_choice!.choice.options[result.index];
        sent.next = result.next;
        sent.script = result.script ?? sent.script;
        give(option);
        keep();
        return result.index;
      }
      if (result.kind === "mismatch") {
        // All or nothing: the answers of the plan are taken back before the player is asked.
        if (!sent.refused) {
          sent.refused = result.reason;
          groups.push({ start: sent.start, end: Number.POSITIVE_INFINITY });
        }
        if (own.length > sent.start) {
          return back(canGoBack);
        }
        groups = groups.filter((item) => item.end !== Number.POSITIVE_INFINITY);
        planStopped = {
          destination: sent.destination,
          applied: 0,
          remaining: sent.script.slice(sent.lead),
          interrupted: false,
          reason: result.reason,
        };
      } else if (result.kind === "interrupted") {
        planStopped = {
          destination: sent.destination,
          applied: done,
          remaining: sent.script.slice(sent.lead + done),
          interrupted: true,
          reason: result.reason,
        };
      }
      if (own.length > sent.start) {
        groups.push({ start: sent.start, end: own.length });
      }
      plan = null;
    }
    return null;
  };
  /**
   * The answer to a choice that needs none from the player: the next step of the plan, or one
   * more step back. Null when the player is asked.
   */
  const decide = (update: SessionUpdate, canGoBack: boolean, goesBack = false): number | null => {
    const index = decidePlan(update, canGoBack);
    if (index !== null) {
      return index;
    }
    if (undoing) {
      if (canUndo() && (goesBack || insideMovement(update))) {
        return back(canGoBack);
      }
      undoing = false;
    }
    return null;
  };
  /** The run that is the game now is no longer it. Where it waits for an answer, it leaves. */
  const leave = () => {
    current += 1;
    pending?.answer(LEAVE);
    pending = null;
  };
  start();

  const step = () => {
    if (pending?.paused) {
      const { answer } = pending;
      pending = null;
      answer(GO);
    }
  };
  // The tools of a local game. The saved game and the scenarios are of the app that made this
  // transport (`onDecisions`, `Engine.scenarios`).
  const capabilities: Capabilities = {
    stepping: {
      set(on) {
        stepping = on;
        tellStepping();
        if (!on) {
          step();
        }
      },
      step,
    },
    switchSeat(seat) {
      const viewer = pending?.update.viewer;
      if (!pending || plan || viewer?.role !== "player") {
        return;
      }
      // Bit 0 is seat a.
      const bit = (id: string) => 1 << (id.charCodeAt(0) - 97);
      const now = switches.at(-1)?.humans ?? game.humans;
      if (now & bit(seat)) {
        return;
      }
      const humans = (now & ~bit(viewer.seat)) | bit(seat);
      const { answer, switchTo } = pending;
      pending = null;
      undoing = false;
      switches.push({ decision: switchTo(humans), humans });
      keep();
      answer(SWITCH);
    },
    wait(ms) {
      waitMs = Math.max(0, ms);
      tellStepping();
    },
    scenarios: true,
    save: true,
  };

  return {
    catalog: engine.catalog,
    odds: (request) => spaceOddsOf(engine.module, request),
    capabilities,
    subscribe(listener) {
      listeners.add(listener);
      listener({ kind: "modes", modes: { ...modes } });
      if (latest) {
        listener(latest);
      }
      return () => listeners.delete(listener);
    },
    submitChoice(nonce, optionId) {
      if (pending?.nonce !== nonce) {
        return;
      }
      const index = pending.options.indexOf(optionId);
      if (index < 0) {
        return;
      }
      const { answer, update } = pending;
      pending = null;
      give(update.pending_choice!.choice.options[index]);
      keep();
      answer(index);
    },
    submitPlan(nonce, sent) {
      this.applyDraft(nonce, { choices: [], plan: sent });
    },
    applyDraft(nonce, draft) {
      if (pending?.nonce !== nonce || pending.paused || pending.update.viewer.role !== "player") {
        return;
      }
      plan = {
        destination: draft.plan?.destination ?? "",
        lead: draft.choices.length,
        script: [
          ...draft.choices.map((option_id): DraftStep => ({ kind: "choose", option_id })),
          ...(draft.plan?.steps ?? []),
        ],
        next: 0,
        actor: pending.update.viewer.seat,
        start: own.length,
        refused: null,
      };
      const { update, answer, canGoBack } = pending;
      const index = decide(update, canGoBack);
      if (index === null) {
        // The first step was refused: nothing was sent, and the same choice is open.
        show(update);
        return;
      }
      pending = null;
      answer(index);
    },
    draft(script, scope) {
      // Later, not now: the answer of the server is later too, and the page has one path.
      queueMicrotask(() => {
        if (!listeners.size) {
          return;
        }
        let outcome: DraftOutcome | null = null;
        if (pending) {
          try {
            outcome = pending.draft(script, scope);
          } catch (error) {
            const reason = error instanceof Error ? error.message : String(error);
            outcome = { update: pending.update, consumed: 0, stop: { kind: "failed", reason } };
          }
        }
        say({ kind: "draft", script, scope, outcome });
      });
    },
    setReactionMode(key, mode) {
      if ((modes[key] ?? "always") === mode) {
        return;
      }
      if (mode === "always") {
        delete modes[key];
      } else {
        modes[key] = mode;
      }
      tellMode(key, mode);
      play.onModes?.({ ...modes });
      say({ kind: "modes", modes: { ...modes } });
      // A window that is open stays as it was asked: the mode is of the windows after it.
    },
    undo() {
      if (!canUndo()) {
        return;
      }
      undoing = true;
      undone = [];
      // With no choice open the game is over: it is played again without the answer.
      const { answer = null, canGoBack = false } = pending ?? {};
      pending = null;
      answer?.(back(canGoBack));
      if (!answer) {
        back(false);
      }
    },
    close() {
      leave();
      listeners.clear();
    },
  };
}
