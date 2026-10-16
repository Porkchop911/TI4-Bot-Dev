import { MemoryBackend, StoreError, openOfflineSaveStore } from "@ti4/shared/storage/index.ts";
import { expect, it } from "vitest";
import { readLeft, storeLeft } from "./savedGame";
import { type SavedGameSummary, importFailure, saveRows, takeLeft } from "./saves";
import type { HostSave } from "./wasmTransport";

const summary = (change: Partial<SavedGameSummary> = {}): SavedGameSummary => ({
  schema: 2,
  id: "g1",
  slot: "single",
  nickname: "Ada",
  createdAt: 0,
  updatedAt: 0,
  status: "in_progress",
  summary: {
    playerCount: 4,
    humans: 1,
    factions: ["hacan", "sol", null, null],
    seed: 12,
    round: 2,
    phase: "action",
    answers: 17,
  },
  engine: { build: "b", contentDigest: "c", wasmSha256: "w" },
  resumeEstimateMs: null,
  imported: false,
  sizeBytes: 100,
  ...change,
});

it("lists a saved game by its kind, where it is and who plays it here", () => {
  const hour = 3_600_000;
  expect(saveRows([summary()], 3 * hour)).toEqual([
    {
      id: "g1",
      href: "/?single",
      title: "Single player",
      detail: "Round 2 · action phase · 4 seats · Hacan · 17 answers",
      updated: "3 h ago",
    },
  ]);
  const [local, scenario, imported] = saveRows([
    summary({ slot: "local.3.8.1" }),
    summary({ slot: "scenario.late-game.5", status: "finished" }),
    summary({ id: "x", slot: undefined, imported: true }),
  ]);
  expect(local.title).toBe("Local game · seed 12");
  expect(local.href).toBe("/?local=3&players=8&humans=1");
  expect(scenario.title).toBe("Scenario late-game");
  expect(scenario.detail).toMatch(/^Finished · /);
  expect(imported.title).toBe("Imported game");
  expect(imported.href).toBe("/?save=x");
});

it("says why a file was not imported, and names a file of an older version", () => {
  const invalid = new StoreError("invalid", "x");
  expect(importFailure(invalid, "{}")).toBe("The file is not a saved game.");
  expect(importFailure(invalid, JSON.stringify({ format: 5, seed: 3 }))).toBe(
    "The file is a saved game of an older version. It cannot be played again.",
  );
  expect(importFailure(invalid, JSON.stringify({ kind: "ti4-offline-game", version: 1 }))).toMatch(
    /older version/,
  );
});

const save = (answers: string[]): HostSave => ({
  format: 2,
  seed: 3,
  players: 4,
  humans: 1,
  game: { local: { seed: 3, players: 4, humans: 1 } },
  decisions: ["x", ...answers],
  answers,
  finished: false,
  engine: { build: "b", content_digest: "c" },
});

function storage() {
  const items = new Map<string, string>();
  return {
    get length() {
      return items.size;
    },
    key: (index: number) => [...items.keys()][index] ?? null,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  };
}

it("puts the save that a left page kept into the store, unless the store has a later one", async () => {
  let now = 100;
  const store = await openOfflineSaveStore({ backend: new MemoryBackend(), now: () => now });
  const local = storage();
  const meta = {
    nickname: "Ada",
    factions: ["sol", null, null, null],
    engine: { build: "b", contentDigest: "c", wasmSha256: "w" },
  };
  // The store has the game with one answer; the page was left with two, before the write ended.
  await store.put("g", save(["a"]), { ...meta, slot: "local.3.4.1" });
  storeLeft(local, "g", { slot: "local.3.4.1", engine: "w", at: 150, save: save(["a", "b"]) });
  // A new game that the store never had.
  storeLeft(local, "n", { slot: "single", engine: "w", at: 150, save: save([]) });
  now = 200;
  await takeLeft(store, local, "Player");
  expect((await store.load("g"))?.save.answers).toEqual(["a", "b"]);
  expect((await store.load("g"))?.summary.factions).toEqual(meta.factions);
  expect((await store.findSlot("single"))?.id).toBe("n");
  expect(readLeft(local, "g")).toBeNull();
  expect(local.length).toBe(0);
  // The store wrote the game after the page was left: its record is newer.
  storeLeft(local, "g", { engine: "w", at: 150, save: save(["old"]) });
  await takeLeft(store, local, "Player");
  expect((await store.load("g"))?.save.answers).toEqual(["a", "b"]);
  expect(local.length).toBe(0);
});
