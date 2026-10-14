import { expect, it } from "vitest";
import {
  SINGLE_ADDRESS,
  SINGLE_KEY,
  type SavedGame,
  clearSavedGame,
  gameAddress,
  loadSavedGame,
  parseSavedGame,
  saveKey,
  storeSavedGame,
} from "./savedGame";
import type { TableRequest } from "./wire";

const game = { seed: 3, players: 8, humans: 1 };
const save: SavedGame = { format: 3, engine: "ab12", ...game, decisions: ["sol", "pass", "18"] };

function storage() {
  const items = new Map<string, string>();
  return {
    items,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  };
}

it("keeps a game under the key of its seed, seats and human seats", () => {
  const store = storage();
  expect(loadSavedGame(store, game)).toBeNull();
  storeSavedGame(store, save);
  expect([...store.items.keys()]).toEqual(["ti4.local.3.8.1"]);
  expect(loadSavedGame(store, game)).toEqual(save);
  expect(loadSavedGame(store, { ...game, seed: 4 })).toBeNull();
  clearSavedGame(store, game);
  expect(loadSavedGame(store, game)).toBeNull();
});

it("drops fields that are not part of a saved game", () => {
  expect(parseSavedGame(JSON.stringify({ ...save, extra: 1 }))).toEqual(save);
});

it("says which part of a saved game is wrong", () => {
  const broken = (change: object) => () => parseSavedGame(JSON.stringify({ ...save, ...change }));
  expect(() => parseSavedGame("{")).toThrow(/^The saved game is not JSON: /);
  expect(() => parseSavedGame("null")).toThrow("The saved game is not a JSON object.");
  expect(broken({ format: 4 })).toThrow("The saved game has format 4; this page reads format 3.");
  expect(broken({ table: { setup: {} } })).toThrow(
    'The saved game has no setup and seats in "table".',
  );
  expect(broken({ waitMs: "1" })).toThrow('The saved game has no whole number in "waitMs".');
  expect(broken({ seed: "3" })).toThrow('The saved game has no whole number in "seed".');
  expect(broken({ humans: -1 })).toThrow('The saved game has no whole number in "humans".');
  expect(broken({ engine: undefined })).toThrow('The saved game has no text in "engine".');
  expect(broken({ decisions: ["a", 2] })).toThrow(
    'The saved game has no list of option ids in "decisions".',
  );
});

it("keeps the seat changes of a game, and says when they are not a list of them", () => {
  const switched = { ...save, switches: [{ decision: 12, humans: 4 }] };
  expect(parseSavedGame(JSON.stringify(switched))).toEqual(switched);
  expect(parseSavedGame(JSON.stringify({ ...save, switches: [] }))).toEqual(save);
  expect(() => parseSavedGame(JSON.stringify({ ...save, switches: [{ decision: 12 }] }))).toThrow(
    'The saved game has no list of seat changes in "switches".',
  );
});

it("makes the key from the game alone", () => {
  expect(saveKey(save)).toBe(saveKey(game));
});

it("keeps a scenario apart from the game of the same seed", () => {
  const scenario = { ...game, scenario: "late-game" };
  expect(saveKey(scenario)).toBe("ti4.local.scenario.late-game.1");
  expect(gameAddress(scenario)).toBe("?scenario=late-game&humans=1");
  expect(gameAddress(game)).toBe("?local=3&players=8&humans=1");
  const store = storage();
  storeSavedGame(store, { ...save, scenario: "late-game" });
  expect(loadSavedGame(store, game)).toBeNull();
  expect(loadSavedGame(store, scenario)).toEqual({ ...save, scenario: "late-game" });
  expect(() => parseSavedGame(JSON.stringify({ ...save, scenario: 3 }))).toThrow(/"scenario"/);
});

it("does not read a game of an older format: it has the answers of the page only", () => {
  for (const format of [1, 2]) {
    const old = { format, engine: "ab12", ...game, answers: ["sol", "pass", "18"] };
    expect(() => parseSavedGame(JSON.stringify(old))).toThrow(
      `The saved game has format ${format}; this page reads format 3.`,
    );
  }
});

it("keeps the game of a table under the key of single player, with its table and its wait", () => {
  const table: TableRequest = {
    setup: { player_count: 4, seed: 12, strategy_cards: "pok" },
    seats: [
      { kind: "human", faction: "hacan" },
      { kind: "smart" },
      { kind: "random" },
      {
        kind: "smart",
      },
    ],
  };
  const single: SavedGame = {
    format: 3,
    engine: "ab12",
    seed: 12,
    players: 4,
    humans: 1,
    table,
    waitMs: 500,
    decisions: ["leadership"],
  };
  const store = storage();
  storeSavedGame(store, single);
  expect([...store.items.keys()]).toEqual([SINGLE_KEY]);
  expect(saveKey(single)).toBe(SINGLE_KEY);
  expect(gameAddress(single)).toBe(SINGLE_ADDRESS);
  // The table says which game it is: any game of a table is read from the one key.
  expect(loadSavedGame(store, { seed: 0, players: 0, humans: 0, table })).toEqual(single);
  expect(parseSavedGame(JSON.stringify(single))).toEqual(single);
});
