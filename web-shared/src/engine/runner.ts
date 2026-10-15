import type { Host } from "./host.ts";
import type { Envelope, HostRestart, HostSave, StartConfig } from "./host.ts";
import type { FromWorker, ModelFile, ToWorker } from "./protocol.ts";

/**
 * The logic of the Worker, apart from `self`: one game at a time, started on a wasm instance,
 * parked at each question, answered by the commands of the page. A history change that replays
 * the game (the reply carries `restart`) is run here: the parked run is left, and a new instance
 * starts with the restart as its save (README, "Commands").
 */
export class HostRunner {
  private module: WebAssembly.Module | null = null;
  private host: Host | null = null;
  private run = 0;
  private save: HostSave | null = null;
  private restart: HostRestart | null = null;
  /**
   * What the game was started with, and the settings that the page changed since (`stepping`,
   * `set_reaction_mode`): a restart starts with them.
   */
  private config: StartConfig | null = null;
  /** The models that every instance of a game loads before its game (a restart too). */
  private models: ModelFile[] = [];

  constructor(
    private readonly post: (message: FromWorker) => void,
    private readonly create: (module: WebAssembly.Module) => Promise<Host>,
  ) {}

  async handle(message: ToWorker): Promise<void> {
    switch (message.t) {
      case "init":
        this.module = message.module;
        this.post({ t: "ready" });
        return;
      case "models":
        this.models = message.models;
        return;
      case "query":
        return this.query(message.id, message.request);
      case "start":
        await this.begin(message.config);
        return;
      case "command":
        this.command(message.id, message.request);
        return;
      case "close":
        this.host = null;
    }
  }

  /**
   * A query (`describe`, `catalog`, the odds, …) needs no game: it is asked of the instance of the
   * game when one is there (also while it waits at a question), else of a new instance.
   */
  private async query(id: number, request: object): Promise<void> {
    try {
      const host = this.host ?? (await this.instance());
      this.post({ t: "reply", id, value: host.query(request) });
    } catch (error) {
      this.post({ t: "reply", id, error: text(error) });
    }
  }

  private async instance(): Promise<Host> {
    if (!this.module) {
      throw new Error("The engine is not loaded");
    }
    return this.create(this.module);
  }

  private async begin(config: StartConfig): Promise<void> {
    const run = ++this.run;
    this.config = config;
    // A command waits for the new run: the old one has left.
    this.host = null;
    let host: Host;
    try {
      host = await this.instance();
      this.load(host);
    } catch (error) {
      this.post({ t: "crash", message: text(error) });
      return;
    }
    this.host = host;
    this.save = null;
    host.onProgress = (progress) => this.post({ t: "progress", progress });
    host.onAsk = (envelope) => {
      if (run !== this.run) {
        return;
      }
      // Cheap (about 20 ms late in a game) and makes every question a place to come back to.
      this.save = this.snapshot(host);
      this.post({ t: "ask", envelope, save: this.save });
    };
    try {
      const end = await host.start(config);
      if (run !== this.run) {
        return;
      }
      const restart = this.restart;
      this.restart = null;
      const save = this.lastSave();
      if (restart && save) {
        // The history change left the parked run; play it again with the new answers.
        await this.begin(this.restarted(save, restart));
        return;
      }
      this.post({ t: "end", envelope: end, save: end.end?.finished ? this.snapshot(host) : null });
    } catch (error) {
      if (run === this.run) {
        this.post({ t: "crash", message: text(error) });
      }
    }
  }

  /** Loads the models into the instance of a game; a file that is not the one expected throws. */
  private load(host: Host): void {
    for (const model of this.models) {
      const loaded = host.loadModel(new Uint8Array(model.bytes));
      if (loaded.sha256 !== model.sha256) {
        throw new Error(
          `The file of the model "${loaded.name}" is not the one that the server lists (SHA-256 ${loaded.sha256}, not ${model.sha256})`,
        );
      }
    }
  }

  /** The start of the run after a history change: the game as it was started, from the restart. */
  private restarted(save: HostSave, restart: HostRestart): StartConfig {
    const { resume: _, setup: _setup, table: _table, local: _local, ...base } = this.config ?? {};
    return {
      ...base,
      // The game with the choices that were left open filled in.
      ...save.game,
      resume: { ...restart, never: save.never } as StartConfig["resume"],
    };
  }

  /** Keeps a setting of the page for the next restart. */
  private remember(request: { type?: string; mode?: unknown; card?: unknown }): void {
    const config = this.config;
    if (!config) {
      return;
    }
    if (request.type === "stepping" && typeof request.mode === "number") {
      this.config = { ...config, stepping: request.mode };
    } else if (
      request.type === "set_reaction_mode" &&
      typeof request.card === "string" &&
      typeof request.mode === "string"
    ) {
      this.config = { ...config, modes: { ...config.modes, [request.card]: request.mode } };
    }
  }

  private lastSave(): HostSave | null {
    return this.save;
  }

  private snapshot(host: Host): HostSave | null {
    try {
      return host.command({ type: "save_state" }).save ?? null;
    } catch {
      return null;
    }
  }

  private command(id: number, request: object): void {
    this.remember(request);
    const host = this.host;
    if (!host) {
      this.post({ t: "reply", id, error: "There is no game running" });
      return;
    }
    try {
      // A history change may restart the game: keep the settings (the Never set) as they are now.
      if ((request as { type?: string }).type === "history") {
        this.save = this.snapshot(host) ?? this.save;
      }
      const envelope: Envelope = host.command(request);
      if (envelope.restart) {
        this.restart = envelope.restart;
      }
      this.post({ t: "reply", id, envelope });
      // The game was over (an undo of its last answer): no parked run leaves, so the history
      // change is played at once.
      const save = this.lastSave();
      if (this.restart && !host.isParked && save) {
        const restart = this.restart;
        this.restart = null;
        void this.begin(this.restarted(save, restart));
      }
    } catch (error) {
      this.post({ t: "reply", id, error: text(error) });
    }
  }
}

const text = (error: unknown): string => (error instanceof Error ? error.message : String(error));
