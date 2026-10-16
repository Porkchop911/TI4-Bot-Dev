import { describe, expect, it } from "vitest";
import type { Envelope, HostSave } from "../engine/host.ts";
import type { FromWorker, HostPort, ToWorker } from "../engine/protocol.ts";
import { engineLink } from "./engineLink.ts";
import type { LinkEvent } from "./link.ts";

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

/** A worker the test drives by hand. `onPost` answers a command or query like the engine would. */
class FakePort implements HostPort {
  posted: ToWorker[] = [];
  terminated = false;
  private listener: (message: FromWorker) => void = () => undefined;
  onPost: (message: ToWorker) => FromWorker | null = () => null;

  post(message: ToWorker): void {
    this.posted.push(message);
    const reply = this.onPost(message);
    if (reply) {
      queueMicrotask(() => this.listener(reply));
    }
  }
  listen(listener: (message: FromWorker) => void): void {
    this.listener = listener;
  }
  terminate(): void {
    this.terminated = true;
  }
  emit(message: FromWorker): void {
    this.listener(message);
  }
}

const CONFIG = { setup: { seed: 7 } };

function open(gate?: (e: Envelope) => void | Promise<void>) {
  const port = new FakePort();
  const link = engineLink(port, CONFIG, gate ? { gate } : {});
  const events: LinkEvent[] = [];
  link.subscribe((event) => events.push(event));
  return { port, link, events };
}

const tick = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("engineLink", () => {
  it("starts the game and tells a question as messages, a save and opened", () => {
    const { port, events } = open();
    expect(port.posted).toEqual([{ t: "start", config: CONFIG }]);
    port.emit({ t: "ready" });
    port.emit({
      t: "ask",
      envelope: envelope("ask", { messages: [{ type: "a" }, { type: "b" }] }),
      save: save(),
    });
    expect(events).toEqual([
      { kind: "message", message: { type: "a" } },
      { kind: "message", message: { type: "b" } },
      { kind: "save", save: save() },
      { kind: "opened" },
    ]);
    port.emit({ t: "ask", envelope: envelope("ask"), save: save(["x"]) });
    expect(events.slice(4)).toEqual([{ kind: "save", save: save(["x"]) }]);
  });

  it("tells progress", () => {
    const { port, events } = open();
    port.emit({ t: "progress", progress: { done: 3, total: 9 } });
    expect(events).toEqual([{ kind: "progress", done: 3, total: 9 }]);
  });

  it("answers a request with the envelope and tells its messages", async () => {
    const { port, link, events } = open();
    port.onPost = (message) =>
      message.t === "command"
        ? {
            t: "reply",
            id: message.id,
            envelope: envelope("command", { messages: [{ type: "ok" }], resume: 1 }),
          }
        : null;
    const reply = await link.request({ type: "submit_choice" });
    expect(reply.resume).toBe(1);
    expect(events).toEqual([{ kind: "message", message: { type: "ok" } }]);
    expect(port.posted[1]).toEqual({ t: "command", id: 1, request: { type: "submit_choice" } });
  });

  it("sends a command without waiting for its reply", async () => {
    const { port, link, events } = open();
    port.onPost = (message) =>
      message.t === "command" ? { t: "reply", id: message.id, error: "refused" } : null;
    link.send({ type: "set_reaction_mode" });
    await tick();
    expect(events).toEqual([]);
  });

  it("rejects a request with the error of its reply", async () => {
    const { port, link } = open();
    port.onPost = (message) =>
      message.t === "command" ? { t: "reply", id: message.id, error: "no game" } : null;
    await expect(link.request({ type: "history" })).rejects.toThrow("no game");
  });

  it("answers a query with its value", async () => {
    const { port, link } = open();
    port.onPost = (message) =>
      message.t === "query" ? { t: "reply", id: message.id, value: { n: 1 } } : null;
    await expect(link.query({ query: "catalog" })).resolves.toEqual({ n: 1 });
  });

  it("resolves the waiter of the next question, and of the end", async () => {
    const { port, link, events } = open();
    const first = link.nextQuestion();
    port.emit({
      t: "ask",
      envelope: envelope("ask", { batch: { status: "applied", applied: 2 } }),
      save: null,
    });
    expect((await first.promise)?.batch?.applied).toBe(2);
    const last = link.nextQuestion();
    port.emit({
      t: "end",
      envelope: envelope("end", { messages: [{ type: "over" }] }),
      save: save(),
    });
    expect((await last.promise)?.kind).toBe("end");
    expect(events.slice(1)).toEqual([
      { kind: "save", save: save() },
      { kind: "message", message: { type: "over" } },
    ]);
  });

  it("does not resolve a cancelled waiter", async () => {
    const { port, link } = open();
    const next = link.nextQuestion();
    let resolved = false;
    void next.promise.then(() => (resolved = true));
    next.cancel();
    port.emit({ t: "ask", envelope: envelope("ask"), save: null });
    await tick();
    expect(resolved).toBe(false);
  });

  it("tells a crash as fatal and rejects what waits", async () => {
    const { port, link, events } = open();
    const next = link.nextQuestion();
    const reply = link.request({ type: "history" });
    port.emit({ t: "crash", message: "unreachable" });
    await expect(next.promise).rejects.toThrow("unreachable");
    await expect(reply).rejects.toThrow("unreachable");
    expect(events).toEqual([{ kind: "fatal", message: "unreachable" }]);
    port.emit({ t: "ask", envelope: envelope("ask"), save: null });
    expect(events).toHaveLength(1);
    await expect(link.request({ type: "history" })).rejects.toThrow("unreachable");
  });

  it("tells an end with an error as fatal", () => {
    const { port, events } = open();
    port.emit({ t: "fatal", message: "no JSPI" });
    expect(events).toEqual([{ kind: "fatal", message: "no JSPI" }]);
    const other = open();
    other.port.emit({
      t: "end",
      envelope: envelope("end", {
        end: { finished: false, left: false, error: "trap", decisions: 0, answers: 0 },
      }),
      save: null,
    });
    expect(other.events).toEqual([{ kind: "fatal", message: "trap" }]);
  });

  it("holds a question and what follows until its gate opens", async () => {
    let open_!: () => void;
    const { port, events } = open(() => new Promise<void>((resolve) => (open_ = resolve)));
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [{ type: "q" }] }), save: save() });
    port.emit({ t: "progress", progress: { done: 1, total: 1 } });
    await tick();
    expect(events).toEqual([]);
    open_();
    await tick();
    expect(events.map((e) => e.kind)).toEqual(["message", "save", "opened", "progress"]);
  });

  it("tells a question at once when its gate returns nothing, and a throwing gate as fatal", () => {
    const seen: Envelope[] = [];
    const { port, events } = open((e) => void seen.push(e));
    port.emit({ t: "ask", envelope: envelope("ask"), save: save() });
    expect(seen).toHaveLength(1);
    expect(events.map((e) => e.kind)).toEqual(["save", "opened"]);

    const refused = open(() => {
      throw new Error("no");
    });
    refused.port.emit({ t: "ask", envelope: envelope("ask"), save: save() });
    expect(refused.events).toEqual([{ kind: "fatal", message: "no" }]);
  });

  it("tells a refused gate as fatal", async () => {
    const { port, events } = open(() => Promise.reject(new Error("deleted")));
    port.emit({ t: "ask", envelope: envelope("ask"), save: save() });
    await tick();
    expect(events).toEqual([{ kind: "fatal", message: "deleted" }]);
  });

  it("closes the port and tells nothing after close", async () => {
    const { port, link, events } = open();
    const next = link.nextQuestion();
    link.close();
    expect(port.terminated).toBe(true);
    await expect(next.promise).rejects.toThrow("closed");
    port.emit({ t: "ask", envelope: envelope("ask"), save: save() });
    expect(events).toEqual([]);
  });
});
