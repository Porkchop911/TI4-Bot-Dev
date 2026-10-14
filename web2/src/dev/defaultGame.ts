import { type LocalGame, gameAddress } from "../session/savedGame";

/** The local game that "Single player" of the start screen opens, and the installed app with it. */
export const DEFAULT_GAME: LocalGame = { seed: 42, players: 8, humans: 1 };

/** Its address. Before the start screen it was the bare address: what it saved is still its game. */
export const SINGLE_PLAYER = `/${gameAddress(DEFAULT_GAME)}`;
