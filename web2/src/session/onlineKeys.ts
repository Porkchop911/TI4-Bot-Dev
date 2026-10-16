// What the browser keeps of the games on the server. `localStorage` is shared by all tabs, so
// everything of a seat is kept for its player: two tabs of one browser can be two seats of one
// game, and the address of a tab says which (`?online=<game_id>&seat=<player_id>`). What a seat
// has staged is kept by the server (`Transport.staged`); a page of before kept it here.
import { NICKNAME_KEY, validNickname } from "@ti4/shared/identity/nickname.ts";
import type { LegacyStaged } from "./seatStore";

/** The key of the name that the player gave last (as in web/), and the names the server takes. */
export { NICKNAME_KEY, validNickname };

/** The part of `Storage` that is used. */
export interface KeyStorage {
  getItem(key: string): string | null;
  setItem(key: string, value: string): void;
  removeItem(key: string): void;
}

/**
 * The name that the player gave last, trimmed, when the server takes it; else null. It is sent
 * when a table is made or joined.
 */
export function keptNickname(storage: Pick<KeyStorage, "getItem">): string | null {
  const name = storage.getItem(NICKNAME_KEY)?.trim();
  return name && validNickname(name) ? name : null;
}

/** The number of seats of the table that this browser made last. */
export const PLAYER_COUNT_KEY = "ti4.players";

const playersKey = (gameId: string) => `ti4.online.${gameId}.players`;
/** Before a browser could have several players in a game: its one player session. */
const legacyKey = (gameId: string) => `ti4.online.${gameId}.session`;

const STAGED = { draft: "movement", plan: "draft", secondary: "secondary" } as const;

/** Where a page of before kept the staged work of one player of a game on the server. */
export const onlineKeys = (gameId: string, playerId: string) => ({
  /** The staged movement, landings or production, with the nonce of its choice. */
  draft: `ti4.online.${gameId}.${playerId}.${STAGED.draft}`,
  /** The private draft of a tactical action. */
  plan: `ti4.online.${gameId}.${playerId}.${STAGED.plan}`,
  /** The private draft of a secondary. */
  secondary: `ti4.online.${gameId}.${playerId}.${STAGED.secondary}`,
});

/** The players that this browser has in a game: the id of each, and its player session. */
export function playersOf(storage: KeyStorage, gameId: string): Record<string, string> {
  try {
    const kept: unknown = JSON.parse(storage.getItem(playersKey(gameId)) ?? "{}");
    if (!kept || typeof kept !== "object" || Array.isArray(kept)) {
      return {};
    }
    return Object.fromEntries(
      Object.entries(kept).filter(
        (entry): entry is [string, string] => typeof entry[1] === "string",
      ),
    );
  } catch {
    return {};
  }
}

/** Keeps a player of this browser in a game. */
export function keepPlayer(storage: KeyStorage, gameId: string, playerId: string, session: string) {
  storage.setItem(
    playersKey(gameId),
    JSON.stringify({ ...playersOf(storage, gameId), [playerId]: session }),
  );
}

/** The player is no longer in the game (it left, or the table removed it). */
export function dropPlayer(storage: KeyStorage, gameId: string, playerId: string) {
  const { [playerId]: _, ...rest } = playersOf(storage, gameId);
  storage.setItem(playersKey(gameId), JSON.stringify(rest));
  for (const key of Object.values(onlineKeys(gameId, playerId))) {
    storage.removeItem(key);
  }
}

/** The player session that an older page kept for the game, with no id of its player. */
export const legacySession = (storage: KeyStorage, gameId: string) =>
  storage.getItem(legacyKey(gameId));

/**
 * Moves what an older page kept for the game to its player, once: the player session, and the
 * staged work, which was kept for the game.
 */
export function moveLegacy(storage: KeyStorage, gameId: string, playerId: string) {
  const session = legacySession(storage, gameId);
  if (session === null) {
    return;
  }
  keepPlayer(storage, gameId, playerId, session);
  storage.removeItem(legacyKey(gameId));
  const keys = onlineKeys(gameId, playerId);
  for (const name of Object.keys(STAGED) as (keyof typeof STAGED)[]) {
    const old = `ti4.online.${gameId}.${STAGED[name]}`;
    const value = storage.getItem(old);
    if (value !== null) {
      storage.setItem(keys[name], value);
      storage.removeItem(old);
    }
  }
}

/**
 * The staged work that a page of before kept for a player, by slot of the server: it is put
 * once, when the server has none, and then dropped (`OnlineGame.legacyStaged`).
 */
export function legacyStaged(storage: KeyStorage, gameId: string, playerId: string): LegacyStaged {
  const keys = onlineKeys(gameId, playerId);
  const slots = { movement: keys.draft, plan: keys.plan, secondary: keys.secondary };
  return {
    read() {
      const values: ReturnType<LegacyStaged["read"]> = {};
      for (const [slot, key] of Object.entries(slots) as [keyof typeof slots, string][]) {
        try {
          const value: unknown = JSON.parse(storage.getItem(key) ?? "null");
          if (value !== null) {
            values[slot] = value;
          }
        } catch {
          // A value that cannot be read is not put.
        }
      }
      return values;
    },
    drop() {
      for (const key of Object.values(slots)) {
        storage.removeItem(key);
      }
    },
  };
}
