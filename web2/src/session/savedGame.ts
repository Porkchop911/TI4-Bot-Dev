// A local game as it is kept between two visits: not its state, but what plays it again.
// The engine is deterministic, so the seed and the decisions of the seats give the same game.
// The server keeps a game the same way: the chosen option of every decision, in order.

import type { TableRequest } from "./wire";

/** Which game the engine sets up. */
export interface LocalGame {
  seed: number;
  /** 3 to 8. */
  players: number;
  /** Bit 0 is the first seat. A seat whose bit is set is played here; the others decide at random. */
  humans: number;
  /**
   * The prepared board that the game starts from (`ti4_view::scenario`). The seed and the number
   * of seats are then those of the scenario.
   */
  scenario?: string;
  /**
   * The table that the game was set up as (single player). The engine starts the game from it:
   * `seed` and `players` are those of its setup, and `humans` is the mask of its players of the
   * kind "human" (bit 0 is the player who came first, with the id "a").
   */
  table?: TableRequest;
  /** The wait of the seats that the page plays before each answer, in milliseconds. */
  waitMs?: number;
}

/** The seats that are played here changed during the game (Dev, "Play as"). */
export interface SeatSwitch {
  /** The number of the decision of the game from which the mask holds (`ti4_decided`). */
  decision: number;
  /**
   * The seats that are played here from then on, as `LocalGame.humans`. An undo does not go back
   * before the change.
   */
  humans: number;
}

export interface SavedGame extends LocalGame {
  /**
   * Formats 1 and 2 had the answers of the page only, and the other seats decided again when
   * the game was played again. They are not read.
   */
  format: 3;
  /** SHA-256 of the engine file that played it. Another engine may offer other options. */
  engine: string;
  /**
   * The option id of every decision of the game, of every seat, in order: `decisions[n]` answers
   * the decision with the number `n`.
   */
  decisions: string[];
  /** In order. Left out when the seats did not change. */
  switches?: SeatSwitch[];
}

/** The part of `Storage` that is used, so a test can pass a map. */
export type SaveStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/** Where the game of single player is kept: one, whatever its table is. */
export const SINGLE_KEY = "ti4.single";

/** The address of single player: the setup of a table, or the game that was started from it. */
export const SINGLE_ADDRESS = "?single";

export const saveKey = (game: LocalGame): string =>
  game.table !== undefined
    ? SINGLE_KEY
    : game.scenario === undefined
      ? `ti4.local.${game.seed}.${game.players}.${game.humans}`
      : `ti4.local.scenario.${game.scenario}.${game.humans}`;

/**
 * The address of a game: what `localGame` in `dev/LocalApp.tsx` reads. The game of a table has
 * the address of its save: a setup does not fit an address.
 */
export const gameAddress = (game: LocalGame): string =>
  game.table !== undefined
    ? SINGLE_ADDRESS
    : game.scenario === undefined
      ? `?local=${game.seed}&players=${game.players}&humans=${game.humans}`
      : `?scenario=${encodeURIComponent(game.scenario)}&humans=${game.humans}`;

/** Where the staged movement of the open movement step is kept: it is not an answer yet. */
export const draftKey = (game: LocalGame): string => `${saveKey(game)}.movement`;

/** Where the private draft of a tactical action is kept, with its undo history. */
export const planKey = (game: LocalGame): string => `${saveKey(game)}.draft`;

/** Where the private draft of a secondary is kept. */
export const secondaryKey = (game: LocalGame): string => `${saveKey(game)}.secondary`;

/** Where the cards that the player is never offered are kept. */
export const cardsKey = (game: LocalGame): string => `${saveKey(game)}.cards`;

/** Whether the other seats wait for a step. A setting of the page, not of one game. */
export const STEPPING_KEY = "ti4.local.stepping";

const isCount = (value: unknown): value is number =>
  Number.isInteger(value) && (value as number) >= 0;

/** Reads a saved game from its JSON text. An error says which part is wrong. */
export function parseSavedGame(text: string): SavedGame {
  let value: Partial<Record<keyof SavedGame, unknown>>;
  try {
    value = JSON.parse(text);
  } catch (error) {
    throw new Error(
      `The saved game is not JSON: ${error instanceof Error ? error.message : error}`,
    );
  }
  if (typeof value !== "object" || value === null) {
    throw new Error("The saved game is not a JSON object.");
  }
  if (value.format !== 3) {
    throw new Error(
      `The saved game has format ${JSON.stringify(value.format)}; this page reads format 3.`,
    );
  }
  for (const field of ["seed", "players", "humans"] as const) {
    if (!isCount(value[field])) {
      throw new Error(`The saved game has no whole number in "${field}".`);
    }
  }
  if (typeof value.engine !== "string") {
    throw new Error('The saved game has no text in "engine".');
  }
  if (value.scenario !== undefined && typeof value.scenario !== "string") {
    throw new Error('The saved game has no text in "scenario".');
  }
  // The engine checks the table when it starts the game: only its shape is read here.
  const table = value.table as Partial<TableRequest> | undefined;
  if (
    table !== undefined &&
    !(
      typeof table === "object" &&
      table !== null &&
      typeof table.setup === "object" &&
      table.setup !== null &&
      Array.isArray(table.seats)
    )
  ) {
    throw new Error('The saved game has no setup and seats in "table".');
  }
  if (value.waitMs !== undefined && !isCount(value.waitMs)) {
    throw new Error('The saved game has no whole number in "waitMs".');
  }
  const { decisions } = value;
  if (!Array.isArray(decisions) || !decisions.every((decision) => typeof decision === "string")) {
    throw new Error('The saved game has no list of option ids in "decisions".');
  }
  const { switches } = value;
  if (
    switches !== undefined &&
    !(
      Array.isArray(switches) &&
      switches.every(
        (item) =>
          typeof item === "object" &&
          item !== null &&
          isCount(item.decision) &&
          isCount(item.humans),
      )
    )
  ) {
    throw new Error('The saved game has no list of seat changes in "switches".');
  }
  return {
    format: 3,
    engine: value.engine,
    seed: value.seed as number,
    players: value.players as number,
    humans: value.humans as number,
    ...(value.scenario === undefined ? {} : { scenario: value.scenario }),
    ...(table === undefined ? {} : { table: table as TableRequest }),
    ...(value.waitMs === undefined ? {} : { waitMs: value.waitMs as number }),
    decisions,
    ...(switches?.length
      ? { switches: switches.map(({ decision, humans }: SeatSwitch) => ({ decision, humans })) }
      : {}),
  };
}

/** The saved game of this seed, seats and human seats, or null when there is none. */
export function loadSavedGame(storage: SaveStorage, game: LocalGame): SavedGame | null {
  const text = storage.getItem(saveKey(game));
  return text === null ? null : parseSavedGame(text);
}

/** Writes the whole game: one short id for each decision, some hundred KiB for a long game of eight. */
export function storeSavedGame(storage: SaveStorage, save: SavedGame): void {
  storage.setItem(saveKey(save), JSON.stringify(save));
}

export function clearSavedGame(storage: SaveStorage, game: LocalGame): void {
  storage.removeItem(saveKey(game));
  storage.removeItem(draftKey(game));
  storage.removeItem(planKey(game));
  storage.removeItem(secondaryKey(game));
  storage.removeItem(cardsKey(game));
}
