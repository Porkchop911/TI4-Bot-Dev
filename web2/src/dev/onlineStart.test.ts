import { expect, it } from "vitest";
import { onlineAddress, onlineStart, tableAddress, tableRequest } from "./onlineStart";

const read = (search: string) => onlineStart(new URLSearchParams(search));

it("the bare address asks for a table of four, and the server chooses the rest", () => {
  expect(read("online")).toEqual({});
  expect(tableRequest(read("online"))).toEqual({ playerCount: 4, nickname: "Player" });
});

it("a table has the seats and the name that the browser used last", () => {
  const kept = { nickname: " Ada ", players: 6 };
  expect(tableRequest(read("online"), kept)).toEqual({ playerCount: 6, nickname: "Ada" });
  // The address has the last word.
  expect(tableRequest(read("online&players=3&name=Blair"), kept)).toEqual({
    playerCount: 3,
    nickname: "Blair",
  });
  // What the browser kept may be anything.
  expect(tableRequest(read("online"), { nickname: "", players: 12 })).toEqual({
    playerCount: 4,
    nickname: "Player",
  });
});

it("the quick start is a game of eight with random seats", () => {
  expect(tableRequest(read("online&fill=random"), { nickname: null, players: 4 })).toEqual({
    playerCount: 8,
    nickname: "Player",
    fill: "random",
  });
  expect(read("online&fill=smart")).toEqual({});
});

it("the address names the seed, the nickname, the seats and the map", () => {
  const start = read("online&seed=3&name=Ada&map=random&players=5");
  expect(start).toEqual({ nickname: "Ada", seed: 3, map: "random", players: 5 });
  expect(tableRequest(start)).toEqual({
    playerCount: 5,
    nickname: "Ada",
    seed: 3,
    map: "random",
  });
});

it("a seed or a count that is no number is left out", () => {
  expect(read("online&seed=abc")).toEqual({});
  expect(read("online&seed=-1&name=%20")).toEqual({});
  expect(read("online&seed=0").seed).toBe(0);
  expect(read("online&players=9")).toEqual({});
  expect(read("online&players=1")).toEqual({});
});

it("the address of a start is read back as that start", () => {
  expect(onlineAddress()).toBe("?online");
  expect(onlineAddress({ seed: 42, fill: "random" })).toBe("?online&fill=random&seed=42");
  const start = { nickname: "Ada Lovelace", seed: 7, map: "random", players: 6 };
  expect(read(onlineAddress(start).slice(1))).toEqual(start);
});

it("the address of a table says which player the tab is", () => {
  expect(tableAddress("g 1")).toBe("?online=g%201");
  expect(tableAddress("g1", "p_2")).toBe("?online=g1&seat=p_2");
});

it("the address names the wait of the seats that the server plays, in seconds", () => {
  expect(read("online&fill=random&wait=1")).toEqual({ fill: "random", wait: 1 });
  expect(tableRequest(read("online&wait=0.5"))).toMatchObject({ waitMs: 500 });
  expect(tableRequest(read("online&wait=0"))).toMatchObject({ waitMs: 0 });
  // The server takes 30 seconds at most.
  expect(read("online&wait=31")).toEqual({});
  expect(read("online&wait=-1")).toEqual({});
  expect(read("online&wait=soon")).toEqual({});
  expect(read(onlineAddress({ fill: "random", wait: 2 }).slice(1))).toEqual({
    fill: "random",
    wait: 2,
  });
});
