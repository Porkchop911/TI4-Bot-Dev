import { expect, it } from "vitest";
import { saveKey } from "../session/savedGame";
import { SINGLE_PLAYER } from "./defaultGame";
import { DEFAULT_GAME, localGame } from "./LocalApp";
import { singleStart } from "./SingleApp";
import { singlePlayer } from "./StartApp";

it("the bare address is no game: it is the start screen", () => {
  expect(localGame(new URLSearchParams(""))).toBeNull();
});

it("the game of before the setup is seed 42, eight seats, and the first seat is played here", () => {
  // The game that the bare address was: what it saved is kept under the same key.
  expect(localGame(new URLSearchParams(SINGLE_PLAYER.slice(2)))).toEqual(DEFAULT_GAME);
  expect(DEFAULT_GAME).toEqual({ seed: 42, players: 8, humans: 1 });
  expect(saveKey(DEFAULT_GAME)).toBe("ti4.local.42.8.1");
});

it("the demo and the gallery keep their own addresses", () => {
  expect(localGame(new URLSearchParams("example=live-picker"))).toBeNull();
  expect(localGame(new URLSearchParams("gallery"))).toBeNull();
});

it("an explicit game is read as before", () => {
  expect(localGame(new URLSearchParams("local=3&players=8&humans=1"))).toEqual({
    seed: 3,
    players: 8,
    humans: 1,
  });
  expect(localGame(new URLSearchParams("example=live-picker&local=5"))).toEqual({
    seed: 5,
    players: 8,
    humans: 1,
  });
});

it("a scenario is a game of its own, and the engine says its seed and seats", () => {
  expect(localGame(new URLSearchParams("scenario=late-game"))).toEqual({
    ...DEFAULT_GAME,
    scenario: "late-game",
  });
  expect(localGame(new URLSearchParams("scenario=late-game&humans=3"))?.humans).toBe(3);
});

it("single player is the setup of a table, or the game that the browser kept", () => {
  const kept = (items: Record<string, string>) => ({
    getItem: (key: string) => items[key] ?? null,
  });
  expect(singlePlayer(kept({}))).toBe("/?single");
  expect(singlePlayer(kept({ "ti4.single": "{}" }))).toBe("/?single");
  // A browser that only has the game of before the setup opens that game.
  expect(singlePlayer(kept({ "ti4.local.42.8.1": "{}" }))).toBe(SINGLE_PLAYER);
  expect(singlePlayer(kept({ "ti4.local.42.8.1": "{}", "ti4.single": "{}" }))).toBe("/?single");
  // `?single` is no game of a seed: the page asks what it kept.
  expect(localGame(new URLSearchParams("single"))).toBeNull();
});

it("the address of single player can name the seed and the seats of a new table", () => {
  expect(singleStart(new URLSearchParams("single"))).toEqual({});
  expect(singleStart(new URLSearchParams("single&seed=3&players=4"))).toEqual({
    seed: 3,
    players: 4,
  });
  expect(singleStart(new URLSearchParams("single&seed=-1&players=9"))).toEqual({});
});
