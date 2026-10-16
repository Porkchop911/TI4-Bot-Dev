import { expect, it } from "vitest";
import { slotOf } from "../session/savedGame";
import { LOCAL_GAME } from "./defaultGame";
import { DEFAULT_GAME, localGame } from "./LocalApp";
import { singleStart } from "./SingleApp";

it("the bare address is no game: it is the start screen", () => {
  expect(localGame(new URLSearchParams(""))).toBeNull();
});

it("the local game of the demo is seed 42, eight seats, and the first seat is played here", () => {
  expect(localGame(new URLSearchParams(LOCAL_GAME.slice(2)))).toEqual(DEFAULT_GAME);
  expect(DEFAULT_GAME).toEqual({ seed: 42, players: 8, humans: 1 });
  expect(slotOf(DEFAULT_GAME)).toBe("local.42.8.1");
});

it("the demo and the gallery keep their own addresses", () => {
  expect(localGame(new URLSearchParams("example=live-strategy-7"))).toBeNull();
  expect(localGame(new URLSearchParams("gallery"))).toBeNull();
});

it("an explicit game is read as before", () => {
  expect(localGame(new URLSearchParams("local=3&players=8&humans=1"))).toEqual({
    seed: 3,
    players: 8,
    humans: 1,
  });
  expect(localGame(new URLSearchParams("example=live-strategy-7&local=5"))).toEqual({
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

it("`?single` is no game of a seed: the page asks the store what it kept", () => {
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
