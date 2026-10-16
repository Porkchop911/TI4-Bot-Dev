import { type LocalGame, gameAddress } from "../session/savedGame";

/** The local game of the header of the demo ("Local game"): seed 42, eight seats, seat A here. */
export const DEFAULT_GAME: LocalGame = { seed: 42, players: 8, humans: 1 };

/** Its address. */
export const LOCAL_GAME = `/${gameAddress(DEFAULT_GAME)}`;
