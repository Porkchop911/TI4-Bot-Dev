import type { MapChoice } from "../protocol/types.ts";
import { PLAYER_COUNTS, type OfflinePlayerCount, type OfflineSetup } from "./types.ts";

export interface SetupDraft {
  playerCount: number;
  /** "random" or a 1-based seat as text. */
  seat: string;
  faction: string | null;
  nickname: string;
  /** The picked map; null = the layout recommended for the table size. */
  map: MapChoice | null;
  /** Raw text of the dev-only seed field; ignored unless `allowSeed`. */
  seed: string;
}

export const DEFAULT_DRAFT: SetupDraft = {
  playerCount: 4,
  seat: "random",
  faction: null,
  nickname: "",
  map: null,
  seed: "",
};

/** Number of players a template alias is made for (`6pStandard` is 6), or null for other names. */
export function aliasPlayers(alias: string): number | null {
  const match = /^(\d+)p/.exec(alias);
  return match ? Number(match[1]) : null;
}

/** A pick made for another table size does not fit a new one; the recommended layout takes over. */
export function mapForPlayers(map: MapChoice | null, playerCount: number): MapChoice | null {
  if (!map || map.kind === "random") return map;
  const made = aliasPlayers(map.alias);
  return made === null || made === playerCount ? map : null;
}

export type SetupResult = { ok: true; setup: OfflineSetup } | { ok: false; error: string };

const isCount = (n: number): n is OfflinePlayerCount => (PLAYER_COUNTS as readonly number[]).includes(n);

/** Validates the form. The seed is honoured only when `allowSeed` (dev builds). */
export function buildSetup(draft: SetupDraft, allowSeed: boolean, pool: readonly string[]): SetupResult {
  if (!isCount(draft.playerCount)) return { ok: false, error: "Choose 3 to 6 players." };
  const nickname = draft.nickname.trim();
  if (!nickname || new TextEncoder().encode(nickname).length > 64)
    return { ok: false, error: "Nickname must be 1 to 64 bytes." };
  let humanSeat: number | null = null;
  if (draft.seat !== "random") {
    humanSeat = Number(draft.seat);
    if (!Number.isInteger(humanSeat) || humanSeat < 1 || humanSeat > draft.playerCount)
      return { ok: false, error: "That seat does not exist at this table size." };
  }
  if (draft.faction !== null && !pool.includes(draft.faction))
    return { ok: false, error: "Unknown faction." };
  const map = mapForPlayers(draft.map, draft.playerCount);
  if (draft.map?.kind === "template" && !draft.map.alias.trim()) return { ok: false, error: "Unknown map." };
  let seed: number | undefined;
  if (allowSeed && draft.seed.trim() !== "") {
    seed = Number(draft.seed);
    if (!Number.isSafeInteger(seed) || seed < 0)
      return { ok: false, error: "Seed must be a non-negative whole number." };
  }
  return {
    ok: true,
    setup: {
      playerCount: draft.playerCount,
      humanSeat,
      humanFaction: draft.faction,
      nickname,
      ...(map ? { map } : {}),
      ...(seed !== undefined ? { seed } : {}),
    },
  };
}

/** "about 12 s", "under a second"; for the resume estimate. */
export function durationLabel(ms: number | null): string {
  if (ms === null) return "a few seconds";
  if (ms < 1000) return "under a second";
  return `about ${Math.round(ms / 1000)} s`;
}

/** Fraction 0..1 for the bar, or null when indeterminate. */
export function progressFraction(done: number, total: number | null): number | null {
  if (total === null || total <= 0) return null;
  return Math.min(1, Math.max(0, done / total));
}
