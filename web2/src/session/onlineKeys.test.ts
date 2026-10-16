import { expect, it } from "vitest";
import {
  dropPlayer,
  keepPlayer,
  legacySession,
  moveLegacy,
  onlineKeys,
  playersOf,
} from "./onlineKeys";

function memory(start: Record<string, string> = {}) {
  const items = new Map(Object.entries(start));
  return {
    items,
    getItem: (key: string) => items.get(key) ?? null,
    setItem: (key: string, value: string) => void items.set(key, value),
    removeItem: (key: string) => void items.delete(key),
  };
}

it("two players of one browser do not share a draft", () => {
  const storage = memory();
  keepPlayer(storage, "g1", "ada", "session-a");
  keepPlayer(storage, "g1", "blair", "session-b");
  expect(playersOf(storage, "g1")).toEqual({ ada: "session-a", blair: "session-b" });
  // Another game of the same browser has its own players.
  expect(playersOf(storage, "g2")).toEqual({});

  const ada = onlineKeys("g1", "ada");
  const blair = onlineKeys("g1", "blair");
  const all = [...Object.values(ada), ...Object.values(blair)];
  expect(new Set(all).size).toBe(all.length);
  storage.setItem(ada.plan, "the draft of Ada");
  expect(storage.getItem(blair.plan)).toBeNull();
});

it("a player that left is forgotten with its staged work", () => {
  const storage = memory();
  keepPlayer(storage, "g1", "ada", "session-a");
  keepPlayer(storage, "g1", "blair", "session-b");
  storage.setItem(onlineKeys("g1", "blair").draft, "staged");
  dropPlayer(storage, "g1", "blair");
  expect(playersOf(storage, "g1")).toEqual({ ada: "session-a" });
  expect(storage.getItem(onlineKeys("g1", "blair").draft)).toBeNull();
});

it("the old key of a game is moved to its player, once", () => {
  const storage = memory({
    "ti4.online.g1.session": "session-a",
    "ti4.online.g1.movement": "staged movement",
    "ti4.online.g1.draft": "private draft",
    "ti4.online.g1.secondary": "secondary draft",
  });
  expect(legacySession(storage, "g1")).toBe("session-a");
  moveLegacy(storage, "g1", "ada");
  expect(playersOf(storage, "g1")).toEqual({ ada: "session-a" });
  expect(legacySession(storage, "g1")).toBeNull();
  const keys = onlineKeys("g1", "ada");
  expect(storage.getItem(keys.draft)).toBe("staged movement");
  expect(storage.getItem(keys.plan)).toBe("private draft");
  expect(storage.getItem(keys.secondary)).toBe("secondary draft");
  expect([...storage.items.keys()].sort()).toEqual(
    ["ti4.online.g1.players", ...Object.values(keys)].sort(),
  );
  // A second run finds nothing to move and changes nothing.
  storage.setItem(keys.plan, "a newer draft");
  moveLegacy(storage, "g1", "blair");
  expect(playersOf(storage, "g1")).toEqual({ ada: "session-a" });
  expect(storage.getItem(keys.plan)).toBe("a newer draft");
});

it("a map that cannot be read is no player", () => {
  expect(playersOf(memory({ "ti4.online.g1.players": "not json" }), "g1")).toEqual({});
  expect(playersOf(memory({ "ti4.online.g1.players": "[1]" }), "g1")).toEqual({});
});
