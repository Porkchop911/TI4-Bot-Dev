import { describe, expect, it } from "vitest";
import type { Envelope, Host, HostSave, StartConfig } from "./host.ts";
import { HostRunner } from "./runner.ts";
import type { FromWorker } from "./protocol.ts";

const envelope = (kind: Envelope["kind"], fields: Partial<Envelope> = {}): Envelope => ({
  kind,
  messages: [],
  ...fields,
});

const save = (answers: string[] = []): HostSave => ({
  format: 2,
  seed: 7,
  players: 3,
  game: { setup: { game_id: "off_test", seed: 7, players: 3, human_seat: 0 } },
  humans: 1,
  decisions: answers,
  answers,
  switches: [],
  redo: [],
  never: [],
  event_times: [],
  event_versions: [],
  version: 1,
  generation: 0,
  decisions_hash: "h",
  finished: false,
  round: 1,
  phase: "action",
  engine: { build: "b", content_digest: "d" },
});

/** A scripted engine: `start` parks at questions the test releases with `answer`. */
class FakeHost {
  onAsk: (e: Envelope) => void = () => undefined;
  onProgress: (p: { done: number; total: number }) => void = () => undefined;
  commands: Record<string, unknown>[] = [];
  private finish!: (e: Envelope) => void;
  started: StartConfig | null = null;
  constructor(
    readonly name: string,
    private readonly onCommand: (host: FakeHost, request: Record<string, unknown>) => Envelope,
  ) {}
  start(config: StartConfig): Promise<Envelope> {
    this.started = config;
    return new Promise((resolve) => (this.finish = resolve));
  }
  command(request: Record<string, unknown>): Envelope {
    this.commands.push(request);
    return this.onCommand(this, request);
  }
  ask(e: Envelope): void {
    this.onAsk(e);
  }
  end(e: Envelope): void {
    this.finish(e);
  }
}

const tick = () => new Promise((r) => setTimeout(r, 0));

describe("HostRunner", () => {
  it("starts a game, attaches the save to each question and forwards progress and commands", async () => {
    const out: FromWorker[] = [];
    const host = new FakeHost("one", (_h, request) =>
      request.type === "save_state"
        ? envelope("command", { save: save(["x"]) })
        : envelope("command", { resume: 0 }),
    );
    const runner = new HostRunner(
      (m) => out.push(m),
      async () => host as unknown as Host,
    );
    await runner.handle({ t: "init", module: {} as WebAssembly.Module });
    expect(out[0]).toEqual({ t: "ready" });
    const config = { setup: { players: 3 } };
    const started = runner.handle({ t: "start", config });
    await tick();
    host.onProgress({ done: 20, total: 40 });
    host.ask(envelope("ask", { messages: [1] }));
    expect(out.find((m) => m.t === "progress")).toEqual({
      t: "progress",
      progress: { done: 20, total: 40 },
    });
    const ask = out.find((m) => m.t === "ask");
    expect(ask).toMatchObject({ t: "ask", save: { answers: ["x"] } });
    await runner.handle({ t: "command", id: 9, request: { type: "submit_choice" } });
    expect(out.at(-1)).toMatchObject({ t: "reply", id: 9, envelope: { resume: 0 } });
    host.end(
      envelope("end", {
        end: { finished: true, left: false, error: null, decisions: 1, answers: 1 },
      }),
    );
    await started;
    expect(out.at(-1)).toMatchObject({ t: "end", save: { answers: ["x"] } });
  });

  it("runs a restart in a new instance with the save's game and the restart as the save", async () => {
    const out: FromWorker[] = [];
    const hosts: FakeHost[] = [];
    const restart = { answers: ["a", "b"], redo: ["c"], generation: 1, version: 7 };
    const make = () => {
      const host = new FakeHost(`h${hosts.length}`, (h, request) => {
        if (request.type === "save_state") {
          return envelope("command", {
            save: { ...save(["a", "b", "c"]), never: ["Shields"], game: { setup: { seed: 7 } } },
          });
        }
        if (request.type === "history") {
          queueMicrotask(() =>
            h.end(
              envelope("end", {
                end: { finished: false, left: true, error: null, decisions: 0, answers: 0 },
              }),
            ),
          );
          return envelope("command", { resume: -1, restart });
        }
        return envelope("command", {});
      });
      hosts.push(host);
      return host;
    };
    const runner = new HostRunner(
      (m) => out.push(m),
      async () => make() as unknown as Host,
    );
    await runner.handle({ t: "init", module: {} as WebAssembly.Module });
    void runner.handle({ t: "start", config: { setup: { seed: 7 } } });
    await tick();
    hosts[0].ask(envelope("ask"));
    await runner.handle({ t: "command", id: 1, request: { type: "history", action: "redo" } });
    await tick();
    await tick();
    expect(hosts).toHaveLength(2);
    expect(hosts[1].started).toEqual({
      setup: { seed: 7 },
      resume: { ...restart, never: ["Shields"] },
    });
    // The page sees the reply, then the next question of the new run; never an end for the old run.
    expect(out.filter((m) => m.t === "end")).toHaveLength(0);
    hosts[1].ask(envelope("ask"));
    expect(out.filter((m) => m.t === "ask")).toHaveLength(2);
  });

  it("restarts a game of a table or a local game as it was started, with the settings changed since", async () => {
    const out: FromWorker[] = [];
    const hosts: FakeHost[] = [];
    const restart = { answers: [], redo: [], generation: 1, version: 7, log: ["a"] };
    const local = { seed: 3, players: 8, humans: 1, bots: [] };
    const make = () => {
      const host = new FakeHost(`h${hosts.length}`, (h, request) => {
        if (request.type === "save_state") {
          return envelope("command", { save: { ...save(["a"]), game: { local } } });
        }
        if (request.type === "undo") {
          queueMicrotask(() =>
            h.end(
              envelope("end", {
                end: { finished: false, left: true, error: null, decisions: 0, answers: 0 },
              }),
            ),
          );
          return envelope("command", { resume: -1, restart });
        }
        return envelope("command", {});
      });
      hosts.push(host);
      return host;
    };
    const runner = new HostRunner(
      (m) => out.push(m),
      async () => make() as unknown as Host,
    );
    await runner.handle({ t: "init", module: {} as WebAssembly.Module });
    void runner.handle({
      t: "start",
      config: { local, modes: { Sabotage: "never" }, stepping: 0, staged: true },
    });
    await tick();
    hosts[0].ask(envelope("ask"));
    await runner.handle({ t: "command", id: 1, request: { type: "stepping", mode: 1 } });
    await runner.handle({
      t: "command",
      id: 2,
      request: { type: "set_reaction_mode", card: "Shields", mode: "own_turn" },
    });
    await runner.handle({ t: "command", id: 3, request: { type: "undo" } });
    await tick();
    await tick();
    expect(hosts).toHaveLength(2);
    expect(hosts[1].started).toEqual({
      local,
      modes: { Sabotage: "never", Shields: "own_turn" },
      stepping: 1,
      staged: true,
      resume: { ...restart, never: [] },
    });
  });

  it("asks a query of the instance of the running game, and of a new one when none runs", async () => {
    const out: FromWorker[] = [];
    const host = new FakeHost("game", () => envelope("command"));
    const queries: string[] = [];
    let created = 0;
    const runner = new HostRunner(
      (m) => out.push(m),
      async () => {
        created += 1;
        const name = created === 1 ? "game" : "spare";
        const target = created === 1 ? host : new FakeHost(name, () => envelope("command"));
        return Object.assign(target, {
          query: (request: object) => {
            queries.push(name);
            return { asked: request };
          },
        }) as unknown as Host;
      },
    );
    await runner.handle({ t: "init", module: {} as WebAssembly.Module });
    const started = runner.handle({ t: "start", config: { setup: {} } });
    await tick();
    host.ask(envelope("ask"));
    await runner.handle({ t: "query", id: 4, request: { query: "space_odds" } });
    expect(out.at(-1)).toEqual({ t: "reply", id: 4, value: { asked: { query: "space_odds" } } });
    await runner.handle({ t: "close" });
    await runner.handle({ t: "query", id: 5, request: { query: "describe" } });
    expect(queries).toEqual(["game", "spare"]);
    host.end(envelope("end"));
    await started;
  });

  it("answers a command with an error when no game runs, and reports a trap as a crash", async () => {
    const out: FromWorker[] = [];
    const runner = new HostRunner(
      (m) => out.push(m),
      async () => {
        throw new Error("instantiate failed");
      },
    );
    await runner.handle({ t: "init", module: {} as WebAssembly.Module });
    await runner.handle({ t: "command", id: 3, request: { type: "ping" } });
    expect(out.at(-1)).toEqual({ t: "reply", id: 3, error: "There is no game running" });
    await runner.handle({ t: "start", config: { setup: {} } });
    expect(out.at(-1)).toEqual({ t: "crash", message: "instantiate failed" });
  });
});
