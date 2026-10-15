import { expect, it } from "vitest";
import {
  SINGLE_ADDRESS,
  SINGLE_SLOT,
  dropOldSaves,
  forgetSide,
  gameAddress,
  gameOfSave,
  clearLeft,
  leftIds,
  readLeft,
  readWait,
  savedAddress,
  sideKey,
  slotOf,
  storeLeft,
  storeWait,
} from "./savedGame";
import type { HostSave } from "./wasmTransport";
import type { TableRequest } from "./wire";

const game = { seed: 3, players: 8, humans: 1 };
const table: TableRequest = {
  setup: { player_count: 4, seed: 12, strategy_cards: "pok" },
  seats: [
    { kind: "human", faction: "hacan" },
    { kind: "smart" },
    { kind: "random" },
    { kind: "smart" },
  ],
};

/** A save of the engine, as far as the page reads it. */
const save = (change: Partial<HostSave> = {}): HostSave => ({
  format: 2,
  seed: 3,
  players: 8,
  humans: 1,
  game: { local: { seed: 3, players: 8, humans: 1, bots: [] } },
  decisions: ["sol", "pass", "18"],
  answers: ["sol"],
  finished: false,
  generation: 2,
  ...change,
});

function storage(items = new Map<string, string>()) {
  return {
    items,
    get length() {
      return items.size;
    },
    key: (index: number) => [...items.keys()][index] ?? null,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  };
}

it("names the slot of a game by its address, and the address by the slot", () => {
  const scenario = { ...game, scenario: "late-game" };
  expect(slotOf(game)).toBe("local.3.8.1");
  expect(slotOf(scenario)).toBe("scenario.late-game.1");
  expect(slotOf({ ...game, table })).toBe(SINGLE_SLOT);
  expect(gameAddress(game)).toBe("?local=3&players=8&humans=1");
  expect(gameAddress(scenario)).toBe("?scenario=late-game&humans=1");
  expect(gameAddress({ ...game, table })).toBe(SINGLE_ADDRESS);
  expect(savedAddress({ id: "x", slot: "local.3.8.1" })).toBe(gameAddress(game));
  expect(savedAddress({ id: "x", slot: "scenario.late-game.1" })).toBe(gameAddress(scenario));
  expect(savedAddress({ id: "x", slot: SINGLE_SLOT })).toBe(SINGLE_ADDRESS);
  // A game with no slot of this app is opened by its id.
  expect(savedAddress({ id: "a b" })).toBe("?save=a%20b");
  expect(savedAddress({ id: "x", slot: "elsewhere" })).toBe("?save=x");
});

it("reads the game of a save: a fixed setup, a scenario, or a table", () => {
  expect(gameOfSave(save())).toEqual(game);
  expect(
    gameOfSave(save({ game: { local: { seed: 42, players: 3, humans: 1, scenario: "x" } } })),
  ).toEqual({ ...game, scenario: "x" });
  expect(gameOfSave(save({ players: 4, game: { table } }))).toEqual({
    seed: 3,
    players: 4,
    humans: 1,
    table,
  });
});

it("keeps the wait, the cards and the save of a page that is left next to a game, and forgets them with it", () => {
  const store = storage();
  expect(readWait(store, "g")).toBe(0);
  storeWait(store, "g", 500);
  expect(readWait(store, "g")).toBe(500);
  store.setItem(sideKey("g", "cards"), "{}");
  store.setItem(sideKey("other", "cards"), "{}");
  storeWait(store, "g", 0);
  expect(store.items.has("ti4.save.g.wait")).toBe(false);
  storeWait(store, "g", 500);
  storeLeft(store, "g", { engine: "ab12", at: 5, save: save() });
  forgetSide(store, "g");
  expect([...store.items.keys()]).toEqual(["ti4.save.other.cards"]);
});

it("keeps the save of a page that is left, and lists the games that have one", () => {
  const store = storage();
  expect(readLeft(store, "g")).toBeNull();
  const left = { slot: "local.3.8.1", engine: "ab12", at: 5, save: save() };
  expect(storeLeft(store, "g", left)).toBe(true);
  expect(readLeft(store, "g")).toEqual(left);
  store.setItem(sideKey("g", "wait"), "500");
  expect(leftIds(store)).toEqual(["g"]);
  // Something else under the key is not read.
  store.setItem(
    sideKey("h", "left"),
    JSON.stringify({ engine: "ab12", at: 5, save: { format: 1 } }),
  );
  expect(readLeft(store, "h")).toBeNull();
  clearLeft(store, "g");
  expect(readLeft(store, "g")).toBeNull();
  // A full localStorage is said, not thrown.
  const full = {
    ...store,
    setItem: () => {
      throw new DOMException("full", "QuotaExceededError");
    },
  };
  expect(storeLeft(full, "g", left)).toBe(false);
});

it("removes the saves of before the save store, and keeps the settings of the page", () => {
  const store = storage(
    new Map([
      ["ti4.single", "{}"],
      ["ti4.single.cards", "[]"],
      ["ti4.local.3.8.1", "{}"],
      ["ti4.local.scenario.late-game.1.cards", "{}"],
      ["ti4.local.stepping", "1"],
      ["ti4.nickname", "Ada"],
      ["ti4.save.g.wait", "500"],
    ]),
  );
  dropOldSaves(store);
  expect([...store.items.keys()]).toEqual([
    "ti4.local.stepping",
    "ti4.nickname",
    "ti4.save.g.wait",
  ]);
});
