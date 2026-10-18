import type { LocalGame } from "../session/savedGame";

/** The local game of a bare `?local` or `?scenario`: seed 42, eight seats, seat A here. */
export const DEFAULT_GAME: LocalGame = { seed: 42, players: 8, humans: 1 };
