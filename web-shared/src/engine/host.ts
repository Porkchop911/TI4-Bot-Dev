/**
 * The hosted game's wasm exports as the Web Worker uses them: a TypeScript port of the reference
 * host `crates/ti4-wasm/js/host.mjs` (read that file and "The hosted game" in
 * `crates/ti4-wasm/README.md` first). Runs in a Worker; it also runs in Node 24 and in the tests.
 *
 * One `Host` is one wasm instance and plays one game. A history change that replays the game
 * (`restart` in the reply) needs a new `Host`.
 */

export { hasJspi } from "./jspi.ts";

/** The answer to a `progress` output: go on. */
const CONTINUE = -5;
export const RESUME_LEAVE = -1;
export const RESUME_UNDO = -2;

export interface HostProgress {
  done: number;
  total: number;
}

/** How a `submit_batch` ended (README, "Commands"). */
export interface HostBatchResult {
  nonce?: string;
  status: "applied" | "refused" | "interrupted";
  applied: number;
  planned?: number;
  message?: string;
  failure?: unknown;
  remaining_steps?: unknown[];
  waiting?: { subtype?: string | null; own_seat?: boolean } | null;
}

export interface HostRestart {
  answers: string[];
  redo: string[];
  generation: number;
  version: number;
  /** Every decision to give again, of every seat (an undo of web2's local play). */
  log?: string[];
  switches?: unknown[];
  undone?: unknown[];
}

/**
 * The game of a request or a save: one of `setup` (the engine's `Setup`), `table` and `local`
 * (`GameRequest` in `crates/ti4-wasm/src/host_setup.rs`).
 */
export interface HostGame {
  setup?: Record<string, unknown>;
  table?: unknown;
  local?: unknown;
}

/**
 * The save of a game (`save_state`, format 2); self-contained: `{resume: save}` plays it again
 * (README of `crates/ti4-wasm`, "Save and resume"). Another format is not read.
 */
export interface HostSave {
  format: 2;
  seed: number;
  /** `setup::PLAYER_COUNTS`, 2 to 8. */
  players: number;
  /** The game, with the choices that were left open filled in. */
  game: HostGame;
  /** The table (`TableSetup`) the game was made from; none for a game of a fixed setup (`local`). */
  table?: Record<string, unknown>;
  scenario?: string;
  /** The seats that the page plays at the start, as a mask (bit 0 is seat `a`). */
  humans: number;
  /** Every decision of the game, of every seat, in order. An undo cuts it. */
  decisions: string[];
  /** The page's answers among them. */
  answers: string[];
  /** The changes of the seats that the page plays: a decision index and a humans mask each. */
  switches: unknown[];
  redo: string[];
  never: string[];
  event_times: unknown[];
  event_versions: unknown[];
  version: number;
  generation: number;
  decisions_hash: string;
  finished: boolean;
  round: number | null;
  phase: string | null;
  engine: { build: string; content_digest: string };
  /** The rule bot version the game plays (left out: a save from before the field, version 1). */
  bot_version?: number;
  /** The rules version the game plays (left out: version 0, the legacy rules). */
  rules_version?: number;
  /** What each seat staged and did not send (`ti4_host::staged`). */
  staged?: Record<string, unknown>;
}

export interface HostEnd {
  finished: boolean;
  left: boolean;
  error: string | null;
  decisions: number;
  answers: number;
  /** Why a saved game did not play again: the decision of its log that the game does not offer. */
  diverged?: string | null;
}

export interface HostResumeCheck {
  ok: boolean;
  [key: string]: unknown;
}

/** What `ti4_host_*` writes. `messages` are `ServerMessage` objects, undecoded. */
export interface Envelope {
  kind: "ask" | "progress" | "end" | "command" | "query";
  messages: unknown[];
  progress?: HostProgress;
  /** An `ask` with no question of the page: the game waits before a decision of this seat (`step`). */
  waiting?: string;
  resume?: number;
  restart?: HostRestart;
  batch?: HostBatchResult;
  save?: HostSave;
  resume_check?: HostResumeCheck;
  end?: HostEnd;
}

/**
 * `ti4_host_start`'s input: one of `setup` (the engine's `Setup`), `table` and `local` (none: the
 * game of the save), and `resume`, a save or a restart. `modes`, `stepping` and `staged` are the
 * settings of the page (`Start` in `crates/ti4-wasm/src/host_setup.rs`).
 */
export interface StartConfig extends HostGame {
  resume?: Partial<HostSave> | HostRestart;
  modes?: Record<string, string>;
  stepping?: number;
  staged?: boolean;
}

interface Exports {
  memory: WebAssembly.Memory;
  ti4_host_in(length: number): number;
  ti4_host_out_ptr(): number;
  ti4_host_out_len(): number;
  ti4_host_set_time(unixMs: number, monotonicMs: number): void;
  ti4_host_query(length: number): number;
  ti4_host_command(length: number): number;
  ti4_host_start(length: number): number;
  ti4_model_load(length: number): number;
}

/** What `ti4_model_load` answers: the model that is loaded, and the SHA-256 of the whole file. */
export interface LoadedModel {
  name: string;
  label: string;
  sha256: string;
  backend: string;
}

const encoder = new TextEncoder();
const decoder = new TextDecoder();

export class Host {
  static async create(module: WebAssembly.Module): Promise<Host> {
    const host = new Host();
    const wa = WebAssembly as unknown as {
      Suspending: new (fn: () => Promise<number>) => unknown;
      promising: (fn: unknown) => (length: number) => Promise<number>;
    };
    const ask = new wa.Suspending(() => host.ask());
    const instance = await WebAssembly.instantiate(module, { host: { ask } as never });
    host.exports = instance.exports as unknown as Exports;
    host.startPromising = wa.promising(host.exports.ti4_host_start);
    return host;
  }

  private exports!: Exports;
  private startPromising!: (length: number) => Promise<number>;
  private parked: { resolve: (code: number) => void } | null = null;
  /** Called with each `ask` envelope; answer by calling {@link command} (which resumes the game). */
  onAsk: (envelope: Envelope) => void = () => undefined;
  /** Called with each `progress` envelope while a save is played again. */
  onProgress: (progress: HostProgress) => void = () => undefined;

  private write(object: unknown): number {
    const bytes = encoder.encode(JSON.stringify(object));
    const pointer = this.exports.ti4_host_in(bytes.length);
    new Uint8Array(this.exports.memory.buffer, pointer, bytes.length).set(bytes);
    return bytes.length;
  }

  /** The last output as text; read again after every call (the buffer can move). */
  private out(): string {
    const pointer = this.exports.ti4_host_out_ptr();
    const length = this.exports.ti4_host_out_len();
    return decoder.decode(new Uint8Array(this.exports.memory.buffer, pointer, length));
  }

  private result(status: number): Envelope {
    const text = this.out();
    if (status < 0) {
      throw new Error(text);
    }
    return JSON.parse(text) as Envelope;
  }

  private time(): void {
    this.exports.ti4_host_set_time(Date.now(), performance.now());
  }

  /** Stateless questions: `describe`, `map_preview`, `catalog`, `space_odds`, `ground_odds`. */
  query<T = unknown>(request: object): T {
    return this.result(this.exports.ti4_host_query(this.write(request))) as unknown as T;
  }

  /**
   * A command (a `ClientMessage`, or `history`, `save_state`, `leave`, `submit_batch`). Returns
   * the `command` envelope; with `resume` set, the parked game goes on with that code.
   */
  command(request: object): Envelope {
    const envelope = this.result(this.exports.ti4_host_command(this.write(request)));
    if (envelope.resume !== undefined && this.parked) {
      const { resolve } = this.parked;
      this.parked = null;
      this.time();
      resolve(envelope.resume);
    }
    return envelope;
  }

  /**
   * Loads an MLP model (the bytes of a `.ti4m` file) into this instance, by its name: a game of
   * this instance with a seat of the kind `mlp:<name>` can start. Throws what the engine refuses.
   */
  loadModel(bytes: Uint8Array): LoadedModel {
    const pointer = this.exports.ti4_host_in(bytes.length);
    new Uint8Array(this.exports.memory.buffer, pointer, bytes.length).set(bytes);
    const status = this.exports.ti4_model_load(bytes.length);
    const text = this.out();
    if (status < 0) {
      throw new Error(text);
    }
    return JSON.parse(text) as LoadedModel;
  }

  /** Plays a game; resolves with the `end` envelope when it ends or is left. */
  async start(config: StartConfig): Promise<Envelope> {
    this.time();
    const status = await this.startPromising(this.write(config));
    return this.result(status);
  }

  /** Whether the game is parked at a question (a command may resume it). */
  get isParked(): boolean {
    return this.parked !== null;
  }

  // The import: the game asks the page. Parks the wasm stack (JSPI) until `command` resumes it.
  private ask(): Promise<number> {
    const envelope = JSON.parse(this.out()) as Envelope;
    if (envelope.kind === "progress") {
      if (envelope.progress) {
        this.onProgress(envelope.progress);
      }
      return Promise.resolve(CONTINUE);
    }
    return new Promise<number>((resolve) => {
      this.parked = { resolve };
      // The page runs here (the stack of the game is parked): it may send commands.
      queueMicrotask(() => this.onAsk(envelope));
    });
  }
}
