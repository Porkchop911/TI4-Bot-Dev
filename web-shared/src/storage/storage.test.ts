import { IDBFactory } from "fake-indexeddb";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { createAutosave } from "./autosave.ts";
import { DB_NAME, MemoryBackend, MIGRATIONS } from "./backend.ts";
import { openOfflineSaveStore } from "./saveStore.ts";
import {
  type EngineIdentity,
  type HostSave,
  LIMITS,
  type OfflineSaveStore,
  StoreError,
} from "./types.ts";
import { engineDifferences } from "./validate.ts";

const ENGINE: EngineIdentity = { build: "b1", contentDigest: "c1", wasmSha256: "w1" };
const META = { nickname: "Ada", factions: [null, "arborec", null, null], engine: ENGINE };

function mkSave(n: number, over: Partial<HostSave> = {}): HostSave {
  return {
    format: 2,
    seed: 42,
    players: 4,
    game: { setup: { seed: 42, players: 4, human_seat: 1 } },
    humans: 0b10,
    decisions: Array.from({ length: n }, (_, i) => `opt-${i}`),
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
    expect(rec?.summary).toMatchObject({
      playerCount: 4,
      humans: 0b10,
      factions: [null, "arborec", null, null],
      answers: 3,
      round: 2,
    });
    expect(rec?.slot).toBeUndefined();
    expect(s2.status()).toMatchObject({ persistence: "indexeddb", warningKind: null });
  });

  it("keeps createdAt, bumps updatedAt, lists newest first", async () => {
    const s = await open();
    await s.put("a", mkSave(1), META);
    await s.put("b", mkSave(1), META);
    await s.put("a", mkSave(2, { finished: true }), META);
    const list = await s.list();
    expect(list.map((g) => g.id)).toEqual(["a", "b"]);
    expect(list[0]!.createdAt).toBeLessThan(list[1]!.createdAt);
    expect(list[0]!.status).toBe("finished");
    expect(list[0]!.summary).toMatchObject({ answers: 2, humans: 0b10 });
    expect(list[0]!.resumeEstimateMs).toBeNull();
  });

  it("finds a game by its slot, and a later write without a slot keeps it", async () => {
    const s = await open();
    await s.put("a", mkSave(1), { ...META, slot: "single" });
    await s.put("b", mkSave(1), { ...META, slot: "local.42.4.2" });
    await s.put("c", mkSave(1), META);
    await s.put("a", mkSave(2), META);
    s.close();
    const s2 = await open();
    expect((await s2.findSlot("single"))?.id).toBe("a");
    expect((await s2.findSlot("single"))?.save.decisions).toHaveLength(2);
    expect((await s2.findSlot("local.42.4.2"))?.id).toBe("b");
    expect(await s2.findSlot("scenario.x.1")).toBeNull();
    await s2.remove("a");
    expect(await s2.findSlot("single")).toBeNull();
  });

  it("refuses factions that do not match the seats", async () => {
    const s = await open();
    await expect(s.put("a", mkSave(1), { ...META, factions: ["sol"] })).rejects.toMatchObject({
      code: "invalid",
    });
    await expect(
      s.put("a", mkSave(1), { ...META, factions: [1, null, null, null] as never }),
    ).rejects.toMatchObject({
      code: "invalid",
    });
    await expect(s.put("a", mkSave(1), { ...META, slot: "" })).rejects.toMatchObject({
      code: "invalid",
    });
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

  it("deletes the records that it does not read, and keeps the others", async () => {
    const backend = new MemoryBackend();
    await backend.putGame({ id: "old", schema: 1 } as never);
    const s = await openOfflineSaveStore({ backend });
    await s.put("ok", mkSave(1), META);
    await s.list();
    expect(s.status().unreadable).toBe(1);
    expect(await s.removeUnreadable()).toBe(1);
    expect(s.status().unreadable).toBe(0);
    expect((await s.list()).map((g) => g.id)).toEqual(["ok"]);
    expect(await backend.getAllGames()).toHaveLength(1);
  });

  it("rejects a malformed save without touching the stored one", async () => {
    const s = await open();
    await s.put("a", mkSave(2), META);
    await expect(s.put("a", { ...mkSave(1), players: 9 }, META)).rejects.toMatchObject({
      code: "invalid",
    });
    expect((await s.load("a"))?.save.answers).toHaveLength(2);
  });

  it("caps the number of games", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    for (let i = 0; i < LIMITS.maxGames; i++) {
      await s.put(`g${i}`, mkSave(0), META);
    }
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
  });

  it("upgrades a version 1 database: the games stay, and the slot index is added", async () => {
    const v1 = await new Promise<IDBDatabase>((res, rej) => {
      const r = factory.open(DB_NAME, 1);
      r.onupgradeneeded = () => MIGRATIONS[0]!(r.result, r.transaction!);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    const old = { id: "old", schema: 1, summary: { humanSeat: 1 } };
    await new Promise<void>((res, rej) => {
      const tx = v1.transaction("games", "readwrite");
      tx.objectStore("games").put(old);
      tx.oncomplete = () => res();
      tx.onerror = () => rej(tx.error);
    });
    v1.close();
    const s = await open();
    expect(s.status()).toMatchObject({ persistence: "indexeddb", warningKind: null });
    // A record of schema 1 is not read: it is counted, not shown.
    expect(await s.list()).toEqual([]);
    expect(s.status().unreadable).toBe(1);
    await s.put("a", mkSave(1), { ...META, slot: "single" });
    s.close();
    const db = await new Promise<IDBDatabase>((res, rej) => {
      const r = factory.open(DB_NAME);
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    expect(db.version).toBe(2);
    const bySlot = await new Promise<unknown>((res, rej) => {
      const r = db.transaction("games").objectStore("games").index("slot").get("single");
      r.onsuccess = () => res(r.result);
      r.onerror = () => rej(r.error);
    });
    expect(bySlot).toMatchObject({ id: "a", slot: "single" });
    db.close();
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
    expect(s.status()).toMatchObject({ warningKind: null, unsynced: 0 });
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
    Object.defineProperty(navigator, "storage", {
      value: {
        persist,
        persisted: async () => false,
        estimate: async () => ({ usage: 5, quota: 10 }),
      },
      configurable: true,
    });
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
    expect(rec?.summary.factions).toEqual(META.factions);
    expect(rec?.slot).toBeUndefined();
    expect((await s.list()).length).toBe(2);
    // The game that is played on stays an imported one.
    await s.put(out.id, mkSave(4), META);
    expect((await s.load(out.id))?.imported).toBe(true);
  });

  it("reports an engine difference", async () => {
    const s = await open();
    const { text } = await exported(s);
    const out = await s.importGame(text, { ...ENGINE, wasmSha256: "other" });
    expect(out.engineDiffers).toBe(true);
    expect(engineDifferences(ENGINE, { ...ENGINE, build: "b2", wasmSha256: "w2" })).toEqual([
      "build",
      "wasmSha256",
    ]);
  });

  it.each([
    ["not json", "{nope"],
    ["wrong kind", JSON.stringify({ kind: "something", version: 1 })],
    ["wrong version", JSON.stringify({ kind: "ti4-offline-game", version: 1, game: {} })],
    ["array", "[]"],
    ["missing game", JSON.stringify({ kind: "ti4-offline-game", version: 2 })],
  ])("rejects %s", async (_n, text) => {
    const s = await open();
    await expect(s.importGame(text)).rejects.toBeInstanceOf(StoreError);
    expect(await s.list()).toEqual([]);
  });

  it("rejects hostile field values in the save", async () => {
    const s = await open();
    const { text } = await exported(s);
    // biome-ignore lint/suspicious/noExplicitAny: a test that writes any value into the file
    const mutate = (fn: (g: Record<string, any>) => void) => {
      const f = JSON.parse(text);
      fn(f.game);
      return JSON.stringify(f);
    };
    for (const bad of [
      mutate((g) => (g.save.players = 9)),
      mutate((g) => (g.save.players = "4")),
      mutate((g) => (g.save.humans = 16)),
      mutate((g) => (g.save.humans = 0)),
      mutate((g) => (g.save.decisions = "x")),
      mutate((g) => (g.save.decisions[0] = 7)),
      mutate((g) => (g.save.seed = -1)),
      mutate((g) => (g.save.seed = 1.5)),
      mutate((g) => (g.save.answers = "x")),
      mutate((g) => (g.save.answers[0] = 7)),
      mutate((g) => (g.save.answers[0] = "y".repeat(500))),
      mutate((g) => (g.save.format = 1)),
      mutate((g) => (g.engine = { build: 1 })),
      mutate((g) => (g.createdAt = "yesterday")),
      mutate((g) => (g.save.game = null)),
      mutate((g) => (g.factions = ["sol"])),
      mutate((g) => delete g.factions),
    ]) {
      await expect(s.importGame(bad)).rejects.toMatchObject({ code: "invalid" });
    }
    expect(await s.list()).toHaveLength(1); // only the original
  });

  it("rejects an oversized file before parsing", async () => {
    const s = await open();
    await expect(s.importGame("x".repeat(LIMITS.maxImportBytes + 1))).rejects.toMatchObject({
      code: "too-large",
    });
    const big = new Blob([new Uint8Array(LIMITS.maxImportBytes + 1)]);
    await expect(s.importGame(big)).rejects.toMatchObject({ code: "too-large" });
  });

  it("strips control characters from an imported nickname and caps its length", async () => {
    const s = await open();
    const { text } = await exported(s);
    const f = JSON.parse(text);
    f.game.nickname = "Eve\u0000\u0007<b>" + "z".repeat(100);
    const out = await s.importGame(JSON.stringify(f));
    // biome-ignore lint/suspicious/noControlCharactersInRegex: it looks for the control characters
    expect(out.record.nickname).not.toMatch(/[\u0000-\u001f]/);
    expect(out.record.nickname.length).toBeLessThanOrEqual(LIMITS.maxNicknameChars);
  });

  it("export of an unknown game is not-found", async () => {
    const s = await open();
    await expect(s.exportGame("zzz")).rejects.toMatchObject({ code: "not-found" });
  });
});

describe("autosave", () => {
  beforeEach(() => vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout", "Date"] }));
  afterEach(() => vi.useRealTimers());

  it("coalesces rapid answers into one write of the newest save", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const put = vi.spyOn(s, "put");
    const a = createAutosave(s, { windowTarget: null, documentTarget: null });
    for (let n = 1; n <= 5; n++) {
      a.schedule("a", mkSave(n), META);
    }
    expect(put).not.toHaveBeenCalled();
    await vi.advanceTimersByTimeAsync(300);
    expect(put).toHaveBeenCalledTimes(1);
    await a.flush();
    expect((await s.load("a"))?.save.answers).toHaveLength(5);
  });

  it("a continuous stream still writes at maxWait", async () => {
    const s = await openOfflineSaveStore({ backend: new MemoryBackend() });
    const put = vi.spyOn(s, "put");
    const a = createAutosave(s, {
      windowTarget: null,
      documentTarget: null,
      debounceMs: 300,
      maxWaitMs: 1000,
    });
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

async function withoutIdb<T>(fn: () => Promise<T>): Promise<T> {
  const g = globalThis as { indexedDB?: unknown };
  const saved = g.indexedDB;
  Object.defineProperty(globalThis, "indexedDB", {
    value: undefined,
    configurable: true,
    writable: true,
  });
  try {
    return await fn();
  } finally {
    Object.defineProperty(globalThis, "indexedDB", {
      value: saved,
      configurable: true,
      writable: true,
    });
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
    if (saved) {
      Object.defineProperty(globalThis, "indexedDB", saved);
    } else {
      delete (globalThis as { indexedDB?: unknown }).indexedDB;
    }
  }
}
