import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import { describe, expect, it } from "vitest";
import { openOfflineSaveStore, type OfflineSaveStore } from "../storage/index.ts";
import { OfflineGameDeletedError, type OfflineMismatchPrompt, type QueuedFeedbackItem } from "../types.ts";
import { OFFLINE_FACTIONS, createLocalLauncher, engineSetup } from "./launcher.ts";
import { FakePort, GAME, envelope, question, save, snapshot } from "./testKit.ts";
import { LocalTransport } from "./LocalTransport.ts";
import { Host, hasJspi } from "./hostApi.ts";
import type { OfflineSetup } from "../types.ts";

const setup: OfflineSetup = { playerCount: 4, humanSeat: 2, humanFaction: "hacan", nickname: "Ada" };

describe("engineSetup", () => {
  it("maps the 1-based seat to the engine's 0-based seat and keeps the nickname out", () => {
    const mapped = engineSetup(setup, "off_x");
    expect(mapped).toMatchObject({ game_id: "off_x", players: 4, human_seat: 1, human_faction: "hacan" });
    expect(JSON.stringify(mapped)).not.toContain("Ada");
    expect(Number.isSafeInteger(mapped.seed)).toBe(true);
  });
  it("leaves a random seat and faction to the engine and honours a dev seed", () => {
    const mapped = engineSetup({ ...setup, humanSeat: null, humanFaction: null, seed: 42 }, "off_x");
    expect(mapped).toEqual({ game_id: "off_x", seed: 42, players: 4 });
  });
});

const feedback = {
  enqueue: async () => ({}) as QueuedFeedbackItem,
  list: () => [],
  subscribe: () => () => undefined,
  retry: () => undefined,
};

const ENGINE = { module: {} as WebAssembly.Module, hash: "abc123", wasmSha256: "w".repeat(64) };

/** A session module whose engine is a FakePort: no wasm, no worker. */
function fakeSession(
  ports: FakePort[],
  saves: ReturnType<typeof save>[],
  extra: { check?: { ok: boolean }; identity?: { build: string; contentDigest: string; wasmSha256: string } } = {},
) {
  return async () =>
    ({
      loadEngine: async () => ENGINE,
      engineIdentity: async () => extra.identity ?? { build: "b", contentDigest: "d", wasmSha256: ENGINE.wasmSha256 },
      openTransport: (_engine: unknown, options: ConstructorParameters<typeof LocalTransport>[0]) =>
        new LocalTransport({
          ...options,
          createPort: async () => {
            const port = new FakePort();
            ports.push(port);
            setTimeout(() => {
              port.emit({
                t: "ask",
                envelope: envelope("ask", {
                  messages: [snapshot(1, "n1")],
                  ...(extra.check ? { resume_check: extra.check } : {}),
                }),
                save: saves[0],
              });
            }, 0);
            return port;
          },
        }),
    }) as unknown as typeof import("./session.ts");
}

const stored = (answers: string[]) => ({
  ...save(answers),
  human_seat: 1,
  setup: { game_id: "ignored", seed: 7, players: 4, human_faction: "hacan" },
});

/** A save store on its own IndexedDB; opening another on the same factory is a page reload. */
const openStore = (factory = new IDBFactory()) => openOfflineSaveStore({ indexedDB: factory });

describe("the local launcher", () => {
  it("creates, autosaves, lists, resumes, exports and removes a game", async () => {
    const store = await openStore();
    const ports: FakePort[] = [];
    const first = stored(["o1", "o2"]);
    const launcher = createLocalLauncher({ store, feedback, session: fakeSession(ports, [first]) });
    expect(launcher.supported()).toEqual(hasJspi() ? { ok: true } : expect.anything());
    expect(launcher.factions).toHaveLength(24);

    const progress: string[] = [];
    const handle = await launcher.create(setup, (p) => progress.push(p.phase), new AbortController().signal);
    expect(handle.gameId).toMatch(/^off_/);
    expect(progress[0]).toBe("loading-engine");
    expect(ports[0].posted[0]).toMatchObject({ t: "start", config: { setup: { human_seat: 1, players: 4, human_faction: "hacan" } } });

    // Autosave went through put(): the record is in IndexedDB with the engine file hash.
    const [summary] = await launcher.list();
    expect(summary).toMatchObject({ playerCount: 3, humanSeat: 2, humanFaction: "hacan", answers: 2, status: "in_progress" });
    const record = await store.load(handle.gameId);
    expect(record?.nickname).toBe("Ada");
    expect(record?.engine).toEqual({ build: "b", contentDigest: "d", wasmSha256: ENGINE.wasmSha256 });

    // The same page opens the same game again: no second engine.
    const again = await launcher.resume(handle.gameId, () => undefined, new AbortController().signal);
    expect(again.transport).toBe(handle.transport);
    expect(ports).toHaveLength(1);

    const file = await launcher.exportGame(handle.gameId);
    const body = JSON.parse(await file.blob.text());
    expect(body).toMatchObject({ kind: "ti4-offline-game", version: 1 });
    expect(JSON.stringify(body)).toContain(ENGINE.wasmSha256);

    await launcher.remove(handle.gameId);
    expect(await launcher.list()).toEqual([]);
    expect(ports[0].terminated).toBe(true);
  });

  it("stores the position before an answer goes to the engine (write-ahead)", async () => {
    const store = await openStore();
    let gated = false;
    let release!: () => void;
    const gate = new Promise<void>((resolve) => (release = resolve));
    const order: string[] = [];
    const slow = Object.create(store) as OfflineSaveStore;
    slow.put = async (...args: Parameters<OfflineSaveStore["put"]>) => {
      if (gated) await gate;
      await store.put(...args);
      order.push("put");
    };
    const ports: FakePort[] = [];
    const launcher = createLocalLauncher({ store: slow, feedback, session: fakeSession(ports, [stored([])]) });
    await store.put(GAME, stored([]) as never, {
      nickname: "Ada",
      humanFaction: "hacan",
      engine: { build: "b", contentDigest: "d", wasmSha256: ENGINE.wasmSha256 },
    });
    const handle = await launcher.resume(GAME, () => undefined, new AbortController().signal);
    const transport = handle.transport as LocalTransport;
    const port = ports[0];
    port.onCommand = (request) => {
      order.push(`engine:${request.type}`);
      setTimeout(() => port.emit({ t: "ask", envelope: envelope("ask", { messages: [question(3, "n3")] }), save: stored(["o1", "o2"]) }), 0);
      return { t: "reply", id: 1, envelope: envelope("command", { resume: 0, messages: [] }) };
    };

    // The engine reaches the next question; its position is being written (slowly) ...
    gated = true;
    order.length = 0;
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [question(2, "n2")] }), save: stored(["o1"]) });
    const answer = transport.submitChoice("o1");
    answer.catch(() => undefined);
    await new Promise((r) => setTimeout(r, 20));
    // ... so the answer has not gone to the engine yet.
    expect(port.commands("submit_choice")).toHaveLength(0);
    release();
    await answer.catch(() => undefined);
    expect(order.indexOf("put")).toBeGreaterThan(-1);
    expect(order.indexOf("put")).toBeLessThan(order.indexOf("engine:submit_choice"));
  });

  it("resumes the same position after a reload, from IndexedDB", async () => {
    const factory = new IDBFactory();
    const first = await openStore(factory);
    const ports: FakePort[] = [];
    const launcher = createLocalLauncher({ store: first, feedback, session: fakeSession(ports, [stored(["a", "b", "c"])]) });
    const handle = await launcher.create(setup, () => undefined, new AbortController().signal);
    first.close();

    // A new page: a new store on the same database, a new launcher, a new engine.
    const second = await openStore(factory);
    const ports2: FakePort[] = [];
    const reloaded = createLocalLauncher({ store: second, feedback, session: fakeSession(ports2, [stored(["a", "b", "c"])]) });
    const phases: string[] = [];
    const again = await reloaded.resume(handle.gameId, (p) => phases.push(`${p.phase}:${p.total}`), new AbortController().signal);
    expect(again.gameId).toBe(handle.gameId);
    expect(ports2[0].posted[0]).toMatchObject({
      t: "start",
      config: { resume: { answers: ["a", "b", "c"] } },
    });
    expect(phases).toContain("replaying:3");
    expect((await second.load(handle.gameId))?.resumeEstimateMs).not.toBeNull();
  });

  it("says so when the game is not there, and honours an abort", async () => {
    const launcher = createLocalLauncher({ store: await openStore(), feedback, session: fakeSession([], [save()]) });
    await expect(launcher.resume("nope", () => undefined, new AbortController().signal)).rejects.toThrow(/not on this device/);
    const controller = new AbortController();
    controller.abort();
    await expect(launcher.create(setup, () => undefined, controller.signal)).rejects.toThrow();
  });

  it("explains a browser without JSPI", async () => {
    const wa = WebAssembly as unknown as { Suspending?: unknown };
    const original = wa.Suspending;
    Object.defineProperty(WebAssembly, "Suspending", { value: undefined, configurable: true, writable: true });
    try {
      const launcher = createLocalLauncher({ store: await openStore(), feedback });
      expect(launcher.supported()).toMatchObject({ ok: false, reason: expect.stringMatching(/Chrome or Edge 137/) });
      await expect(launcher.create(setup, () => undefined, new AbortController().signal)).rejects.toThrow(/JSPI/);
    } finally {
      Object.defineProperty(WebAssembly, "Suspending", { value: original, configurable: true, writable: true });
    }
  });
});

describe("a saved game of another engine", () => {
  /** A game saved by `wasm-old` that this page (engine `w...w`) is asked to resume. */
  async function savedByOldEngine() {
    const factory = new IDBFactory();
    const store = await openStore(factory);
    await store.put(GAME, stored(["a", "b"]) as never, {
      nickname: "Ada",
      humanFaction: "hacan",
      engine: { build: "b", contentDigest: "d", wasmSha256: "old".repeat(20) },
    });
    return store;
  }

  it("asks before replaying anything; view only plays nothing and writes nothing", async () => {
    const store = await savedByOldEngine();
    const before = (await store.load(GAME))!;
    const ports: FakePort[] = [];
    const launcher = createLocalLauncher({ store, feedback, session: fakeSession(ports, [stored(["a", "b"])]) });
    const prompts: OfflineMismatchPrompt[] = [];
    const handle = await launcher.resume(GAME, () => undefined, new AbortController().signal, {
      decide: async (prompt) => {
        prompts.push(prompt);
        expect(ports).toHaveLength(0); // not replayed yet
        return "view-only";
      },
    });
    expect(prompts).toHaveLength(1);
    expect(prompts[0]).toMatchObject({ reason: "engine-changed", differs: ["wasmSha256"] });
    expect(handle.viewOnly).toBe(true);
    const transport = handle.transport as LocalTransport;
    await expect(transport.submitChoice("o1")).rejects.toThrow(/look at only/);
    expect((await store.load(GAME))!.updatedAt).toBe(before.updatedAt);
    expect((await store.load(GAME))!.engine.wasmSha256).toBe(before.engine.wasmSha256);
  });

  it("try anyway plays on and stores the game with the engine now used", async () => {
    const store = await savedByOldEngine();
    const launcher = createLocalLauncher({ store, feedback, session: fakeSession([], [stored(["a", "b"])]) });
    const handle = await launcher.resume(GAME, () => undefined, new AbortController().signal, {
      decide: async () => "try-anyway",
    });
    expect(handle.viewOnly).toBeUndefined();
    expect((await store.load(GAME))!.engine.wasmSha256).toBe(ENGINE.wasmSha256);
  });

  it("delete removes the game and replays nothing", async () => {
    const store = await savedByOldEngine();
    const ports: FakePort[] = [];
    const launcher = createLocalLauncher({ store, feedback, session: fakeSession(ports, [stored([])]) });
    await expect(
      launcher.resume(GAME, () => undefined, new AbortController().signal, { decide: async () => "delete" }),
    ).rejects.toBeInstanceOf(OfflineGameDeletedError);
    expect(await store.load(GAME)).toBeNull();
    expect(ports).toHaveLength(0);
  });

  it("without a way to ask it refuses and changes nothing", async () => {
    const store = await savedByOldEngine();
    const launcher = createLocalLauncher({ store, feedback, session: fakeSession([], [stored([])]) });
    await expect(launcher.resume(GAME, () => undefined, new AbortController().signal)).rejects.toThrow(/another version/);
    expect(await store.load(GAME)).not.toBeNull();
  });

  it("asks again after the replay when the decisions came out different", async () => {
    // Same engine file, but the replay diverges (resume_check.ok false).
    const store = await openStore();
    await store.put(GAME, stored(["a", "b"]) as never, {
      nickname: "Ada",
      humanFaction: "hacan",
      engine: { build: "b", contentDigest: "d", wasmSha256: ENGINE.wasmSha256 },
    });
    const launcher = createLocalLauncher({
      store,
      feedback,
      session: fakeSession([], [stored(["a", "b"])], { check: { ok: false } }),
    });
    const seen: OfflineMismatchPrompt[] = [];
    const handle = await launcher.resume(GAME, () => undefined, new AbortController().signal, {
      decide: async (prompt) => {
        seen.push(prompt);
        return "view-only";
      },
    });
    expect(seen.map((p) => p.reason)).toEqual(["replay-diverged"]);
    expect(handle.viewOnly).toBe(true);
  });
});

describe("the engine file", () => {
  const wasm = resolve(process.cwd(), "src/offline/local/engine/ti4_wasm.wasm");
  it.skipIf(!existsSync(wasm) || !hasJspi())("offers exactly the factions the launcher lists", async () => {
    const host = await Host.create(await WebAssembly.compile(readFileSync(wasm)));
    const form = host.query<{ faction_groups: { factions: { id: string }[] }[] }>({ query: "describe" });
    const ids = form.faction_groups.flatMap((group) => group.factions.map((f) => f.id));
    expect([...OFFLINE_FACTIONS].sort()).toEqual([...ids].sort());
  });
});

