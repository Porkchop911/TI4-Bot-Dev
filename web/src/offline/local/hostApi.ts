/**
 * The hosted game's wasm exports as the Web Worker uses them: a TypeScript port of the reference
 * host `crates/ti4-wasm/js/host.mjs` (read that file and "The hosted game" in
 * `crates/ti4-wasm/README.md` first). Runs in a Worker; it also runs in Node 24 and in the tests.
 *
 * One `Host` is one wasm instance and plays one game. A history change that replays the game
 * (`restart` in the reply) needs a new `Host`.
 */

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
}

/** The save of a game (`save_state`); self-contained, plays again with `resume`. */
export interface HostSave {
  format: number;
  game_id: string;
  seed: number;
  players: number;
  human_seat: number;
  setup: Record<string, unknown>;
  answers: string[];
  redo: string[];
  never: string[];
  event_times: unknown[];
  event_versions: unknown[];
  version: number;
  generation: number;
  decisions: number;
  decisions_hash: string;
  finished: boolean;
  round: number | null;
  phase: string | null;
  engine: { build: string; content_digest: string };
}

export interface HostEnd {
  finished: boolean;
  left: boolean;
  error: string | null;
  decisions: number;
  answers: number;
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
  resume?: number;
  restart?: HostRestart;
  batch?: HostBatchResult;
  save?: HostSave;
  resume_check?: HostResumeCheck;
  end?: HostEnd;
}

/** `ti4_host_start`'s input. `setup` is the engine's `Setup`; `resume` a save or a restart. */
export interface StartConfig {
  setup: Record<string, unknown>;
  resume?: Partial<HostSave> | HostRestart;
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
}

/** JSPI (WebAssembly.Suspending / promising); Chrome and Edge 137 and later, Node 24. */
export function hasJspi(): boolean {
  const wa = WebAssembly as unknown as Record<string, unknown>;
  return typeof wa.Suspending === "function" && typeof wa.promising === "function";
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
    if (status < 0) throw new Error(text);
    return JSON.parse(text) as Envelope;
  }

  private time(): void {
    this.exports.ti4_host_set_time(Date.now(), performance.now());
  }

  /** Stateless questions of the setup form: `describe`, `map_preview`, `catalog`. */
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
      if (envelope.progress) this.onProgress(envelope.progress);
      return Promise.resolve(CONTINUE);
    }
    return new Promise<number>((resolve) => {
      this.parked = { resolve };
      // The page runs here (the stack of the game is parked): it may send commands.
      queueMicrotask(() => this.onAsk(envelope));
    });
  }
}
