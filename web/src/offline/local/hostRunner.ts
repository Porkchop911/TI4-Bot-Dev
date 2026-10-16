import type { Host } from "./hostApi.ts";
import type { Envelope, HostRestart, HostSave, StartConfig } from "./hostApi.ts";
import type { FromWorker, ToWorker } from "./hostProtocol.ts";

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

  private async query(id: number, request: object): Promise<void> {
    try {
      const host = await this.instance();
      this.post({ t: "reply", id, value: host.query(request) });
    } catch (error) {
      this.post({ t: "reply", id, error: text(error) });
    }
  }

  private async instance(): Promise<Host> {
    if (!this.module) throw new Error("The engine is not loaded");
    return this.create(this.module);
  }

  private async begin(config: StartConfig): Promise<void> {
    const run = ++this.run;
    let host: Host;
    try {
      host = await this.instance();
    } catch (error) {
      this.post({ t: "crash", message: text(error) });
      return;
    }
    this.host = host;
    this.save = null;
    host.onProgress = (progress) => this.post({ t: "progress", progress });
    host.onAsk = (envelope) => {
      if (run !== this.run) return;
      // Cheap (about 20 ms late in a game) and makes every question a place to come back to.
      this.save = this.snapshot(host);
      this.post({ t: "ask", envelope, save: this.save });
    };
    try {
      const end = await host.start(config);
      if (run !== this.run) return;
      const restart = this.restart;
      this.restart = null;
      const save = this.lastSave();
      if (restart && save) {
        // The history change left the parked run; play it again with the new answers.
        await this.begin({
          setup: save.setup,
          resume: { ...restart, never: save.never } as StartConfig["resume"],
        });
        return;
      }
      this.post({ t: "end", envelope: end, save: end.end?.finished ? this.snapshot(host) : null });
    } catch (error) {
      if (run === this.run) this.post({ t: "crash", message: text(error) });
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
    const host = this.host;
    if (!host) {
      this.post({ t: "reply", id, error: "There is no game running" });
      return;
    }
    try {
      // A history change may restart the game: keep the settings (the Never set) as they are now.
      if ((request as { type?: string }).type === "history") this.save = this.snapshot(host) ?? this.save;
      const envelope: Envelope = host.command(request);
      if (envelope.restart) this.restart = envelope.restart;
      this.post({ t: "reply", id, envelope });
    } catch (error) {
      this.post({ t: "reply", id, error: text(error) });
    }
  }
}

const text = (error: unknown): string => (error instanceof Error ? error.message : String(error));
