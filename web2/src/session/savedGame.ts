// A local game as it is kept between two visits: not its state, but what plays it again.
// The engine is deterministic, so the seed and the decisions of the seats give the same game.
// The server keeps a game the same way: the chosen option of every decision, in order.
//
// The games are kept in the save store of `web-shared/src/storage/` (IndexedDB `ti4-offline`,
// the same list as web/), each under an id and, for a game of an address of this app, a slot.
// Next to a game, localStorage keeps what the page reads at once: `ti4.save.<id>.<part>`.

import type { SavedGameSummary } from "@ti4/shared/storage/index.ts";
import type { HostSave } from "./wasmTransport";
import type { TableRequest } from "./wire";

/** Which game the engine sets up. */
export interface LocalGame {
  seed: number;
  /** 3 to 8. */
  players: number;
  /** Bit 0 is the first seat. A seat whose bit is set is played here; the others decide at random. */
  humans: number;
  /**
   * The prepared board that the game starts from (`ti4_host::view::scenario`). The seed and the number
   * of seats are then those of the scenario.
   */
  scenario?: string;
  /**
   * The table that the game was set up as (single player). The engine starts the game from it:
   * `seed` and `players` are those of its setup, and `humans` is the mask of its players of the
   * kind "human" (bit 0 is the first seat, with the id "a").
   */
  table?: TableRequest;
  /** The wait of the seats that the page plays before each answer, in milliseconds. */
  waitMs?: number;
}

/** The part of `Storage` that is used, so a test can pass a map. */
export type SaveStorage = Pick<Storage, "getItem" | "setItem" | "removeItem">;

/** The slot of single player: one game, whatever its table is. */
export const SINGLE_SLOT = "single";

/** The address of single player: the setup of a table, or the game that was started from it. */
export const SINGLE_ADDRESS = "?single";

/** The slot of a game of an address of this app (`SavedGameRecord.slot`). */
export const slotOf = (game: LocalGame): string =>
  game.table !== undefined
    ? SINGLE_SLOT
    : game.scenario === undefined
      ? `local.${game.seed}.${game.players}.${game.humans}`
      : `scenario.${game.scenario}.${game.humans}`;

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

/** The address of a saved game that has no slot of this app (an import, a game of web/). */
export const idAddress = (id: string) => `?save=${encodeURIComponent(id)}`;

/** The address of a saved game: the address of its slot, or else of its id. */
export function savedAddress(saved: Pick<SavedGameSummary, "id" | "slot">): string {
  const slot = saved.slot ?? "";
  if (slot === SINGLE_SLOT) {
    return SINGLE_ADDRESS;
  }
  const local = /^local\.(\d+)\.(\d+)\.(\d+)$/.exec(slot);
  if (local) {
    return gameAddress({ seed: +local[1], players: +local[2], humans: +local[3] });
  }
  const scenario = /^scenario\.(.+)\.(\d+)$/.exec(slot);
  if (scenario) {
    return gameAddress({ seed: 0, players: 0, humans: +scenario[2], scenario: scenario[1] });
  }
  return idAddress(saved.id);
}

/** The game that a save plays, as this app names it. */
export function gameOfSave(save: HostSave): LocalGame {
  const game = save.game as { table?: TableRequest; local?: { scenario?: string } };
  const seats = { seed: save.seed, players: save.players, humans: save.humans };
  if (game.table) {
    return { ...seats, table: game.table };
  }
  const scenario = game.local?.scenario;
  return scenario === undefined ? seats : { ...seats, scenario };
}

/** What localStorage keeps next to a saved game: the parts that the page reads or writes at once. */
export type SidePart = "cards" | "wait" | "left";
const SIDE_PARTS: SidePart[] = ["cards", "wait", "left"];

export const sideKey = (id: string, part: SidePart) => `ti4.save.${id}.${part}`;

/** Removes what localStorage keeps next to a saved game: the game is gone. */
export function forgetSide(storage: SaveStorage, id: string) {
  for (const part of SIDE_PARTS) {
    storage.removeItem(sideKey(id, part));
  }
}

/** The wait of a saved game, in milliseconds (0: none kept). */
export function readWait(storage: SaveStorage, id: string): number {
  const value = Number(storage.getItem(sideKey(id, "wait")) || 0);
  return Number.isInteger(value) && value > 0 ? value : 0;
}

export function storeWait(storage: SaveStorage, id: string, ms: number) {
  if (ms > 0) {
    storage.setItem(sideKey(id, "wait"), String(ms));
  } else {
    storage.removeItem(sideKey(id, "wait"));
  }
}

/**
 * The newest save of a game when the page was left, with what was staged then: a write of the
 * save store does not end when the page is gone, a write of localStorage does. It is removed when
 * the store has the save.
 */
export interface LeftSave {
  /** The slot of the game, for a game that the store does not have yet. */
  slot?: string;
  /** SHA-256 of the engine file that played it. */
  engine: string;
  /** When the page was left (`Date.now()`): a record of the store that was written later is newer. */
  at: number;
  /** The faction of each seat, as the page knew it. */
  factions?: Array<string | null>;
  save: HostSave;
}

/** Keeps the save of a page that is left. Says whether localStorage took it. */
export function storeLeft(storage: SaveStorage, id: string, left: LeftSave): boolean {
  try {
    storage.setItem(sideKey(id, "left"), JSON.stringify(left));
    return true;
  } catch {
    // Full: the write of the store is all there is.
    return false;
  }
}

export const clearLeft = (storage: SaveStorage, id: string) =>
  storage.removeItem(sideKey(id, "left"));

/** The save that the page kept when it was left, or null. */
export function readLeft(storage: Pick<Storage, "getItem">, id: string): LeftSave | null {
  try {
    const left: Partial<LeftSave> | null = JSON.parse(
      storage.getItem(sideKey(id, "left")) ?? "null",
    );
    const save = left?.save;
    return left &&
      typeof left.engine === "string" &&
      save?.format === 2 &&
      Array.isArray(save.decisions)
      ? (left as LeftSave)
      : null;
  } catch {
    return null;
  }
}

/** The ids of the games whose save a page kept when it was left. */
export function leftIds(storage: Pick<Storage, "length" | "key">): string[] {
  const ids: string[] = [];
  for (let index = 0; index < storage.length; index++) {
    const id = /^ti4\.save\.(.+)\.left$/.exec(storage.key(index) ?? "")?.[1];
    if (id !== undefined) {
      ids.push(id);
    }
  }
  return ids;
}

/** Whether the other seats wait for a step. A setting of the page, not of one game. */
export const STEPPING_KEY = "ti4.local.stepping";

/**
 * Removes the saves of before the save store: `ti4.single` and `ti4.local.<…>` with their cards.
 * They are not read. Called at each start; there is nothing to remove after the first.
 */
export function dropOldSaves(storage: Pick<Storage, "length" | "key" | "removeItem">) {
  const old: string[] = [];
  for (let index = 0; index < storage.length; index++) {
    const key = storage.key(index);
    if (
      key !== null &&
      key !== STEPPING_KEY &&
      (key === "ti4.single" || key.startsWith("ti4.single.") || key.startsWith("ti4.local."))
    ) {
      old.push(key);
    }
  }
  for (const key of old) {
    storage.removeItem(key);
  }
}
