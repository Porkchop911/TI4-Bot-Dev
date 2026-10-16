import "fake-indexeddb/auto";
import { IDBFactory } from "fake-indexeddb";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createAutosave } from "./autosave.ts";
import { MemoryBackend } from "./backend.ts";
import { engineMismatchPrompt, replayDivergedPrompt } from "./engineCheck.ts";
import { backoffDelay, createDurableFeedbackQueue, httpFeedbackSender } from "./feedbackQueue.ts";
import type { OfflineFeedbackLocal } from "../types.ts";
import { openOfflineSaveStore } from "./saveStore.ts";
import {
  type EngineIdentity,
  type FeedbackSendResult,
  type HostSave,
  LIMITS,
  type OfflineSaveStore,
  StoreError,
} from "./types.ts";

const ENGINE: EngineIdentity = { build: "b1", contentDigest: "c1", wasmSha256: "w1" };
const META = { nickname: "Ada", humanFaction: "arborec", engine: ENGINE };

function mkSave(n: number, over: Partial<HostSave> = {}): HostSave {
  return {
    format: 1,
    game_id: "g",
    seed: 42,
    players: 4,
    human_seat: 1,
    setup: { seed: 42, players: 4 },
    answers: Array.from({ length: n }, (_, i) => `opt-${i}`),
    finished: false,
    round: 2,
    phase: "action",
    ...over,
  };
}

let factory: IDBFactory;
let clock = 1000;
let ids = 0;
const open = (f: IDBFactory = factory) =>
  openOfflineSaveStore({ indexedDB: f, now: () => ++clock, newId: () => `id-${++ids}` });

beforeEach(() => {
  factory = new IDBFactory();
  clock = 1000;
  ids = 0;
});

describe("games", () => {
  it("round-trips a save and survives reopening", async () => {
    const s = await open();
    await s.put("a", mkSave(3), META);
    s.close();
    const s2 = await open();
    const rec = await s2.load("a");
    expect(rec?.save.answers).toEqual(["opt-0", "opt-1", "opt-2"]);
    expect(rec?.engine).toEqual(ENGINE);
    expect(rec?.summary).toMatchObject({ playerCount: 4, humanSeat: 2, answers: 3, round: 2, humanFaction: "arborec" });
    expect(s2.status()).toMatchObject({ persistence: "indexeddb", warning: null });
  });

  it("keeps createdAt, bumps updatedAt, lists newest first, maps to the shell's summary", async () => {
    const s = await open();
    await s.put("a", mkSave(1), META);
    await s.put("b", mkSave(1), META);
    await s.put("a", mkSave(2, { finished: true }), META);
    const list = await s.list();
    expect(list.map((g) => g.id)).toEqual(["a", "b"]);
    expect(list[0]!.createdAt).toBeLessThan(list[1]!.createdAt);
    expect(list[0]!.status).toBe("finished");
    const sum = await s.listSummaries();
    expect(sum[0]).toMatchObject({ gameId: "a", answers: 2, status: "finished", humanSeat: 2, resumeEstimateMs: null });
  });

  it("removes a game", async () => {
    const s = await open();
    await s.put("a", mkSave(1), META);
    await s.remove("a");
    expect(await s.load("a")).toBeNull();
    expect(await s.list()).toEqual([]);
  });

  it("hides records that fail validation and counts them", async () => {
    const backend = new MemoryBackend();
    await backend.putGame({ id: "junk", schema: 99 } as never);
    const s = await openOfflineSaveStore({ backend });
    await s.put("ok", mkSave(1), META);
    expect((await s.list()).map((g) => g.id)).toEqual(["ok"]);
    expect(s.status().unreadable).toBe(1);
  });

  it("rejects a malformed save without touching the stored one", async () => {
    const s = await open();
    await s.put("a", mkSave(2), META);
    await expect(s.put("a", { ...mkSave(1), players: 9 }, META)).rejects.toMatchObject({ code: "invalid" });
    expect((await s.load("a"))?.save.answers).toHaveLength(2);
  });

  it("caps the number of games", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    for (let i = 0; i < LIMITS.maxGames; i++) await s.put(`g${i}`, mkSave(0), META);
    await expect(s.put("extra", mkSave(0), META)).rejects.toMatchObject({ code: "too-many-games" });
    await s.put("g0", mkSave(1), META); // an existing game can still be saved
  });

  it("rejects a save over the size cap", async () => {
    const s = await open();
    const big = mkSave(0, { filler: "x".repeat(LIMITS.maxSaveBytes) });
    await expect(s.put("a", big, META)).rejects.toMatchObject({ code: "too-large" });
  });

  it("serialises concurrent writes of one game: the last one wins", async () => {
    const s = await open();
    await Promise.all([1, 2, 3, 4, 5].map((n) => s.put("a", mkSave(n), META)));
    expect((await s.load("a"))?.save.answers).toHaveLength(5);
  });

  it("migrates: reopening with the same version keeps data; a newer database falls back with a warning", async () => {
    const s = await open();
    await s.put("a", mkSave(1), META);
    s.close();
    // Make the stored database newer than this build.
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = factory.open("ti4-offline", 7);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    db.close();
    const s2 = await open();
    expect(s2.status()).toMatchObject({ persistence: "memory", warningKind: "newer-version" });
    expect(s2.status().warning).toMatch(/newer version/);
  });

  it("upgrades from an empty database (version 0) creating all stores", async () => {
    const s = await open();
    await s.put("a", mkSave(1), META);
    await s.feedbackBackend.put({ localId: "f", reportId: "r" } as never);
    expect(await s.feedbackBackend.getAll()).toHaveLength(1);
  });
});

describe("storage failures", () => {
  it("falls back to memory with a warning when IndexedDB is absent", async () => {
    // fake-indexeddb/auto provides one globally; force absence:
    const none = await withoutIdb(() => openOfflineSaveStore());
    expect(none.status()).toMatchObject({ persistence: "memory", warningKind: "unavailable" });
    await none.put("a", mkSave(2), META);
    expect((await none.load("a"))?.save.answers).toHaveLength(2);
  });

  it("falls back when opening throws (private window)", async () => {
    const throwing = {
      open() {
        throw new DOMException("denied", "SecurityError");
      },
    } as unknown as IDBFactory;
    const s = await open(throwing);
    expect(s.status()).toMatchObject({ persistence: "memory", warningKind: "unavailable" });
    await s.put("a", mkSave(1), META);
    expect(await s.list()).toHaveLength(1);
  });

  it("falls back when the indexedDB accessor itself throws", async () => {
    const s = await withThrowingAccessor(() => openOfflineSaveStore());
    expect(s.status().persistence).toBe("memory");
  });

  it("a quota error keeps the game in memory, raises the warning, and recovers on the next good write", async () => {
    const backend = new MemoryBackend();
    const s = await openOfflineSaveStore({ backend, now: () => ++clock });
    await s.put("a", mkSave(1), META);
    const real = backend.putGame.bind(backend);
    backend.putGame = async () => {
      throw new DOMException("full", "QuotaExceededError");
    };
    await s.put("a", mkSave(2), META); // does not throw: the game goes on
    expect(s.status()).toMatchObject({ warningKind: "quota", unsynced: 1 });
    expect((await s.load("a"))?.save.answers).toHaveLength(2); // served from memory
    expect((await s.list())[0]!.summary.answers).toBe(2);
    backend.putGame = real;
    await s.put("a", mkSave(3), META);
    expect(s.status()).toMatchObject({ warning: null, unsynced: 0 });
    expect((await backend.getAllGames()) as { save: HostSave }[]).toHaveLength(1);
  });

  it("a generic write failure gives write-failed", async () => {
    const backend = new MemoryBackend();
    backend.putGame = async () => {
      throw new Error("disk");
    };
    const s = await openOfflineSaveStore({ backend });
    await s.put("a", mkSave(1), META);
    expect(s.status().warningKind).toBe("write-failed");
  });

  it("notifies subscribers when the warning changes", async () => {
    const backend = new MemoryBackend();
    const s = await openOfflineSaveStore({ backend });
    const cb = vi.fn();
    s.subscribe(cb);
    backend.putGame = async () => {
      throw new Error("x");
    };
    await s.put("a", mkSave(1), META);
    expect(cb).toHaveBeenCalled();
  });

  it("deleting while the backend fails hides the game for this session", async () => {
    const backend = new MemoryBackend();
    const s = await openOfflineSaveStore({ backend });
    await s.put("a", mkSave(1), META);
    backend.deleteGame = async () => {
      throw new Error("x");
    };
    await s.remove("a");
    expect(await s.list()).toEqual([]);
  });

  it("requests persistence once and tolerates a missing API", async () => {
    const s = await open();
    const persist = vi.fn().mockResolvedValue(true);
    Object.defineProperty(navigator, "storage", { value: { persist, persisted: async () => false, estimate: async () => ({ usage: 5, quota: 10 }) }, configurable: true });
    expect(await s.requestPersistence()).toBe(true);
    await s.requestPersistence();
    expect(persist).toHaveBeenCalledTimes(1);
    expect(await s.estimate()).toEqual({ usage: 5, quota: 10 });
    Object.defineProperty(navigator, "storage", { value: undefined, configurable: true });
    const s2 = await open(new IDBFactory());
    expect(await s2.requestPersistence()).toBeNull();
    expect(await s2.estimate()).toBeNull();
  });
});

describe("export and import", () => {
  async function exported(s: OfflineSaveStore) {
    await s.put("a", mkSave(3), { ...META, nickname: "Ada L." });
    const { filename, blob } = await s.exportGame("a");
    return { filename, text: await blob.text() };
  }

  it("round-trips through a file into a fresh id", async () => {
    const s = await open();
    const { filename, text } = await exported(s);
    expect(filename).toMatch(/^ti4-offline-ada-l-a\.json$/);
    const out = await s.importGame(new Blob([text]), ENGINE);
    expect(out.id).not.toBe("a");
    expect(out.engineDiffers).toBe(false);
    const rec = await s.load(out.id);
    expect(rec?.imported).toBe(true);
    expect(rec?.save.answers).toEqual(["opt-0", "opt-1", "opt-2"]);
    expect((await s.list()).length).toBe(2);
  });

  it("reports an engine difference", async () => {
    const s = await open();
    const { text } = await exported(s);
    const out = await s.importGame(text, { ...ENGINE, wasmSha256: "other" });
    expect(out.engineDiffers).toBe(true);
  });

  it.each([
    ["not json", "{nope"],
    ["wrong kind", JSON.stringify({ kind: "something", version: 1 })],
    ["wrong version", JSON.stringify({ kind: "ti4-offline-game", version: 2, game: {} })],
    ["array", "[]"],
    ["missing game", JSON.stringify({ kind: "ti4-offline-game", version: 1 })],
  ])("rejects %s", async (_n, text) => {
    const s = await open();
    await expect(s.importGame(text)).rejects.toBeInstanceOf(StoreError);
    expect(await s.list()).toEqual([]);
  });

  it("rejects hostile field values in the save", async () => {
    const s = await open();
    const { text } = await exported(s);
    const mutate = (fn: (g: Record<string, any>) => void) => {
      const f = JSON.parse(text);
      fn(f.game);
      return JSON.stringify(f);
    };
    for (const bad of [
      mutate((g) => (g.save.players = 2)),
      mutate((g) => (g.save.players = "4")),
      mutate((g) => (g.save.human_seat = 4)),
      mutate((g) => (g.save.seed = -1)),
      mutate((g) => (g.save.seed = 1.5)),
      mutate((g) => (g.save.answers = "x")),
      mutate((g) => (g.save.answers[0] = 7)),
      mutate((g) => (g.save.answers[0] = "y".repeat(500))),
      mutate((g) => (g.save.format = 2)),
      mutate((g) => (g.engine = { build: 1 })),
      mutate((g) => (g.createdAt = "yesterday")),
      mutate((g) => (g.save.setup = null)),
    ]) {
      await expect(s.importGame(bad)).rejects.toMatchObject({ code: "invalid" });
    }
    expect(await s.list()).toHaveLength(1); // only the original
  });

  it("rejects an oversized file before parsing", async () => {
    const s = await open();
    await expect(s.importGame("x".repeat(LIMITS.maxImportBytes + 1))).rejects.toMatchObject({ code: "too-large" });
    const big = new Blob([new Uint8Array(LIMITS.maxImportBytes + 1)]);
    await expect(s.importGame(big)).rejects.toMatchObject({ code: "too-large" });
  });

  it("strips control characters from an imported nickname and caps its length", async () => {
    const s = await open();
    const { text } = await exported(s);
    const f = JSON.parse(text);
    f.game.nickname = "Eve\u0000\u0007<b>" + "z".repeat(100);
    const out = await s.importGame(JSON.stringify(f));
    expect(out.record.nickname).not.toMatch(/[\u0000-\u001f]/);
    expect(out.record.nickname.length).toBeLessThanOrEqual(LIMITS.maxNicknameChars);
  });

  it("export of an unknown game is not-found", async () => {
    const s = await open();
    await expect(s.exportGame("zzz")).rejects.toMatchObject({ code: "not-found" });
  });
});

describe("engine check", () => {
  it("is silent when identical and lists the differences otherwise", () => {
    expect(engineMismatchPrompt(ENGINE, ENGINE)).toBeNull();
    const p = engineMismatchPrompt(ENGINE, { ...ENGINE, build: "b2", wasmSha256: "w2" })!;
    expect(p.reason).toBe("engine-changed");
    expect(p.differs).toEqual(["build", "wasmSha256"]);
    expect(p.choices.map((c) => c.id)).toEqual(["view-only", "try-anyway", "delete"]);
    expect(p.choices.find((c) => c.id === "delete")!.destructive).toBe(true);
  });

  it("resume_check.ok false gives the replay-diverged prompt", () => {
    expect(replayDivergedPrompt({ ok: true }, ENGINE, ENGINE)).toBeNull();
    expect(replayDivergedPrompt(undefined, ENGINE, ENGINE)).toBeNull();
    const p = replayDivergedPrompt({ ok: false }, ENGINE, ENGINE)!;
    expect(p.reason).toBe("replay-diverged");
    expect(p.choices).toHaveLength(3);
  });
});

describe("autosave", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] }));
  afterEach(() => vi.useRealTimers());

  it("coalesces rapid answers into one write of the newest save", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const put = vi.spyOn(s, "put");
    const a = createAutosave(s, { windowTarget: null, documentTarget: null });
    for (let n = 1; n <= 5; n++) a.schedule("a", mkSave(n), META);
    expect(put).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(300);
    expect(put).toHaveBeenCalledTimes(1);
    await a.flush();
    expect((await s.load("a"))?.save.answers).toHaveLength(5);
  });

  it("a continuous stream still writes at maxWait", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const put = vi.spyOn(s, "put");
    const a = createAutosave(s, { windowTarget: null, documentTarget: null, debounceMs: 300, maxWaitMs: 1000 });
    for (let n = 1; n <= 20; n++) {
      a.schedule("a", mkSave(n), META);
      await vi.advanceTimersByTimeAsync(200);
    }
    expect(put.mock.calls.length).toBeGreaterThanOrEqual(3);
  });

  it("flush writes at once, and pagehide / hidden tab flush", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const win = new EventTarget();
    const doc = Object.assign(new EventTarget(), { visibilityState: "visible" });
    const a = createAutosave(s, { windowTarget: win, documentTarget: doc });
    a.schedule("a", mkSave(1), META);
    win.dispatchEvent(new Event("pagehide"));
    await a.flush();
    expect(await s.load("a")).not.toBeNull();
    a.schedule("b", mkSave(1), META);
    doc.visibilityState = "hidden";
    doc.dispatchEvent(new Event("visibilitychange"));
    await a.flush();
    expect(await s.load("b")).not.toBeNull();
  });

  it("reports write errors through onError and keeps going", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const onError = vi.fn();
    const a = createAutosave(s, { windowTarget: null, documentTarget: null, onError });
    a.schedule("a", { ...mkSave(1), players: 1 } as HostSave, META);
    await a.flush();
    expect(onError).toHaveBeenCalledTimes(1);
    a.schedule("a", mkSave(1), META);
    await a.flush();
    expect(await s.load("a")).not.toBeNull();
  });
});

const LOCAL: OfflineFeedbackLocal = {
  seed: 1,
  setup: { players: 4 },
  answers: ["x"],
  engine: { build: "0.1.0", content_digest: "dig" },
  wasm_sha256: "ab".repeat(32),
  save: {
    seed: 1, setup: { players: 4 }, answers: ["x"], redo: [], never: [], event_times: [], event_versions: [],
    generation: 0, version: 1, decisions: 1, decisions_hash: "h",
  },
};

describe("feedback queue", () => {
  const report = { category: "bug" as const, text: "broken", context: { a: 1 }, local: LOCAL };

  async function setup(results: FeedbackSendResult[] | ((n: number) => FeedbackSendResult), store?: OfflineSaveStore) {
    const s = store ?? (await openOfflineSaveStore({ backend: new MemoryBackend() }));
    let t = 10_000;
    let n = 0;
    const sent: unknown[] = [];
    const q = createDurableFeedbackQueue({
      backend: s.feedbackBackend,
      send: async (p) => {
        sent.push(p);
        const r = typeof results === "function" ? results(n) : (results[n] ?? results.at(-1)!);
        n++;
        return r;
      },
      now: () => t,
      random: () => 0.5,
      newId: () => `q${++ids}`,
      newReportId: () => `report_${ids}`,
      baseDelayMs: 1000,
      maxDelayMs: 8000,
      maxAttempts: 4,
      onlineTarget: null,
    });
    return { s, q, sent, advance: (ms: number) => (t += ms), clock: () => t };
  }

  it("sends in the background and marks sent", async () => {
    const { q, sent } = await setup([{ ok: true, serverId: "srv1" }]);
    await q.start();
    const item = await q.enqueue(report);
    expect(item.state).toBe("queued");
    await q.flush();
    expect(sent).toHaveLength(1);
    expect((sent[0] as { reportId: string }).reportId).toBeTruthy();
    expect(q.list()[0]).toMatchObject({ state: "sent", serverId: "srv1", attempts: 1 });
    q.dispose();
  });

  it("retries with backoff, then succeeds", async () => {
    const { q, sent, advance } = await setup([
      { ok: false, retryable: true, message: "offline" },
      { ok: false, retryable: true, message: "offline" },
      { ok: true, serverId: "s" },
    ]);
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "queued", attempts: 1 });
    await q.flush(); // not due yet
    expect(sent).toHaveLength(1);
    advance(1000);
    await q.flush();
    expect(q.list()[0]!.attempts).toBe(2);
    advance(1000); // backoff is now 2000
    await q.flush();
    expect(sent).toHaveLength(2);
    advance(1000);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "sent", attempts: 3 });
    expect(new Set(sent.map((p) => (p as { reportId: string }).reportId)).size).toBe(1); // same id every time
    q.dispose();
  });

  it("a non-retryable answer fails at once; retry() resets it", async () => {
    const { q, sent } = await setup((n) => (n === 0 ? { ok: false, retryable: false, message: "HTTP 400" } : { ok: true, serverId: "s" }));
    await q.start();
    const item = await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "failed" });
    q.retry(item.localId);
    await vi.waitFor(() => expect(q.list()[0]!.state).toBe("sent"));
    expect(sent).toHaveLength(2);
    q.dispose();
  });

  it("gives up after maxAttempts", async () => {
    const { q, advance } = await setup([{ ok: false, retryable: true, message: "x" }]);
    await q.start();
    await q.enqueue(report);
    for (let i = 0; i < 6; i++) {
      await q.flush();
      advance(10_000);
    }
    expect(q.list()[0]).toMatchObject({ state: "failed", attempts: 4 });
    q.dispose();
  });

  it("a sender that throws counts as a retryable failure", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const q = createDurableFeedbackQueue({ backend: s.feedbackBackend, send: async () => { throw new Error("boom"); }, onlineTarget: null });
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]).toMatchObject({ state: "queued", attempts: 1 });
    q.dispose();
  });

  it("is durable: items survive a restart, and a crash while sending is re-sent with the same report id", async () => {
    const backend = new MemoryBackend();
    const s = await openOfflineSaveStore({ backend });
    const hang = setup([{ ok: true, serverId: "s" }], s);
    const first = await hang;
    // Simulate a crash mid-send: a sender that never answers.
    const q1 = createDurableFeedbackQueue({ backend: s.feedbackBackend, send: () => new Promise(() => undefined), onlineTarget: null, newId: () => "k1", newReportId: () => "k1_report" });
    await q1.start();
    await q1.enqueue(report);
    await vi.waitFor(async () => expect((await s.feedbackBackend.getAll())[0]!.state).toBe("sending"));
    q1.dispose();
    first.q.dispose();
    const s2 = await openOfflineSaveStore({ backend });
    const sent: string[] = [];
    const q2 = createDurableFeedbackQueue({
      backend: s2.feedbackBackend,
      send: async (p) => {
        sent.push(p.reportId);
        return { ok: true, serverId: "s" };
      },
      onlineTarget: null,
    });
    await q2.start();
    await q2.flush();
    expect(sent).toEqual(["k1_report"]); // reportId == the one created before the crash
    expect(q2.list()[0]!.state).toBe("sent");
    q2.dispose();
  });

  it("enqueue with a known reportId is idempotent", async () => {
    const { q } = await setup([{ ok: false, retryable: true, message: "x" }]);
    await q.start();
    const a = await q.enqueue({ ...report, reportId: "R_report_1" });
    const b = await q.enqueue({ ...report, reportId: "R_report_1" });
    expect(b.localId).toBe(a.localId);
    expect(q.list()).toHaveLength(1);
    q.dispose();
  });

  it("takes the report id the caller made and keeps it across restarts", async () => {
    const { q, sent } = await setup([{ ok: true, serverId: "s" }]);
    await q.start();
    await q.enqueue({ ...report, reportId: "r_0123456789ab" });
    await q.flush();
    expect((sent[0] as { reportId: string }).reportId).toBe("r_0123456789ab");
    q.dispose();
  });

  it("validates and caps reports", async () => {
    const { q } = await setup([{ ok: true, serverId: "s" }]);
    await q.start();
    await expect(q.enqueue({ ...report, text: "  " })).rejects.toMatchObject({ code: "invalid" });
    await expect(q.enqueue({ ...report, text: "x".repeat(LIMITS.maxFeedbackTextChars + 1) })).rejects.toMatchObject({ code: "invalid" });
    await expect(q.enqueue({ ...report, context: { blob: "x".repeat(LIMITS.maxFeedbackPayloadBytes) } })).rejects.toMatchObject({ code: "too-large" });
    await expect(q.enqueue({ ...report, category: "nope" as never })).rejects.toMatchObject({ code: "invalid" });
    q.dispose();
  });

  it("limits the number of waiting reports", async () => {
    const { q } = await setup([{ ok: false, retryable: true, message: "x" }]);
    await q.start();
    for (let i = 0; i < LIMITS.maxFeedbackItems; i++) await q.enqueue(report);
    await expect(q.enqueue(report)).rejects.toBeInstanceOf(StoreError);
    q.dispose();
  });

  it("notifies subscribers with new snapshots (identity changes)", async () => {
    const { q } = await setup([{ ok: true, serverId: "s" }]);
    await q.start();
    const cb = vi.fn();
    const off = q.subscribe(cb);
    const before = q.list();
    await q.enqueue(report);
    expect(cb).toHaveBeenCalled();
    expect(q.list()).not.toBe(before);
    off();
    q.dispose();
  });

  it("works from memory when the database cannot be written", async () => {
    const backend = new MemoryBackend();
    backend.putFeedback = async () => {
      throw new DOMException("full", "QuotaExceededError");
    };
    const s = await openOfflineSaveStore({ backend });
    const { q } = await setup([{ ok: true, serverId: "s" }], s);
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]!.state).toBe("sent");
    expect(s.status().warningKind).toBe("quota");
    q.dispose();
  });

  it("drops old sent items on start", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    await s.feedbackBackend.put({ localId: "old", reportId: "r", report, state: "sent", attempts: 1, queuedAt: 1, nextAttemptAt: 0 });
    const q = createDurableFeedbackQueue({ backend: s.feedbackBackend, send: async () => ({ ok: true, serverId: "" }), now: () => 10 * 24 * 3_600_000, onlineTarget: null });
    await q.start();
    expect(q.list()).toEqual([]);
    expect(await s.feedbackBackend.getAll()).toEqual([]);
    q.dispose();
  });

  it("online event makes queued items due now", async () => {
    const target = new EventTarget();
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    let ok = false;
    let t = 0;
    const q = createDurableFeedbackQueue({
      backend: s.feedbackBackend,
      send: async () => (ok ? { ok: true, serverId: "s" } : { ok: false, retryable: true, message: "offline" }),
      now: () => t,
      onlineTarget: target,
    });
    await q.start();
    await q.enqueue(report);
    await q.flush();
    expect(q.list()[0]!.state).toBe("queued");
    ok = true;
    target.dispatchEvent(new Event("online"));
    await vi.waitFor(() => expect(q.list()[0]!.state).toBe("sent"));
    q.dispose();
  });

  it("backoff grows, is capped, and jitters", () => {
    expect(backoffDelay(1, 1000, 8000, 0.5)).toBe(1000);
    expect(backoffDelay(3, 1000, 8000, 0.5)).toBe(4000);
    expect(backoffDelay(9, 1000, 8000, 0.5)).toBe(8000);
    expect(backoffDelay(1, 1000, 8000, 0)).toBe(750);
    expect(backoffDelay(1, 1000, 8000, 1)).toBe(1250);
  });
});

async function withoutIdb<T>(fn: () => Promise<T>): Promise<T> {
  const g = globalThis as { indexedDB?: unknown };
  const saved = g.indexedDB;
  Object.defineProperty(globalThis, "indexedDB", { value: undefined, configurable: true, writable: true });
  try {
    return await fn();
  } finally {
    Object.defineProperty(globalThis, "indexedDB", { value: saved, configurable: true, writable: true });
  }
}

async function withThrowingAccessor<T>(fn: () => Promise<T>): Promise<T> {
  const saved = Object.getOwnPropertyDescriptor(globalThis, "indexedDB");
  Object.defineProperty(globalThis, "indexedDB", {
    configurable: true,
    get() {
      throw new DOMException("blocked", "SecurityError");
    },
  });
  try {
    return await fn();
  } finally {
    if (saved) Object.defineProperty(globalThis, "indexedDB", saved);
  }
}

describe("the feedback sender follows the server contract", () => {
  const payload = { reportId: "r_0123456789ab", category: "bug" as const, text: "t", context: { a: 1 }, local: LOCAL };
  const reply = (status: number, body: unknown = {}) =>
    vi.fn(async (_url: string, _init?: RequestInit) => new Response(JSON.stringify(body), { status }));

  it("posts report_id at the top level and the replay data in context.local, no header", async () => {
    const fetchImpl = reply(200, { id: "fb_1", duplicate: false });
    const result = await httpFeedbackSender(fetchImpl as unknown as typeof fetch)(payload);
    expect(result).toEqual({ ok: true, serverId: "fb_1" });
    const [url, init] = fetchImpl.mock.calls[0]!;
    expect(url).toBe("/api/feedback");
    expect(Object.keys(init!.headers as Record<string, string>)).toEqual(["content-type"]);
    const body = JSON.parse(init!.body as string);
    expect(body).toEqual({
      category: "bug",
      text: "t",
      context: { a: 1, local: LOCAL },
      report_id: "r_0123456789ab",
    });
    expect(body.context.offline).toBeUndefined();
  });

  it("counts duplicate:true as sent", async () => {
    const result = await httpFeedbackSender(reply(200, { id: "fb_1", duplicate: true }) as unknown as typeof fetch)(payload);
    expect(result).toEqual({ ok: true, serverId: "fb_1" });
  });

  it("retries 429 and 5xx, gives up on 400, retries a network error", async () => {
    for (const status of [429, 500, 503])
      expect(await httpFeedbackSender(reply(status) as unknown as typeof fetch)(payload)).toMatchObject({ ok: false, retryable: true });
    expect(await httpFeedbackSender(reply(400) as unknown as typeof fetch)(payload)).toMatchObject({ ok: false, retryable: false });
    const down = vi.fn(async () => { throw new TypeError("Failed to fetch"); });
    expect(await httpFeedbackSender(down as unknown as typeof fetch)(payload)).toMatchObject({ ok: false, retryable: true });
  });
});
