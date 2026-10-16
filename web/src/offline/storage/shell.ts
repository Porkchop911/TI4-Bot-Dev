/** What web's offline shell reads from the shared save store: its list and its warning text. */
import type { OfflineGameSummary } from "../types.ts";
import type { OfflineSaveStore, SavedGameSummary, StoreStatus } from "@ti4/shared/storage/index.ts";

const WARNINGS: Record<NonNullable<StoreStatus["warningKind"]>, string> = {
  unavailable:
    "Saving to this browser is not available (a private window, or storage is blocked). " +
    "Your game is kept only while this tab stays open; export it to keep a copy.",
  "newer-version":
    "The saved games on this device were written by a newer version of the app. " +
    "They cannot be opened here, and new games are kept only while this tab stays open.",
  quota: "This device is out of storage space, so the latest moves are not saved. Free up space or delete old games.",
  "write-failed": "The game could not be saved to this browser just now. It will try again after your next move.",
};

/** The text of the visible notice for a store's status; null when all is well. */
export const storageWarning = (status: StoreStatus): string | null =>
  status.warningKind ? WARNINGS[status.warningKind] : null;

/** Why a listed game does not open here: the game screen shows one seat. */
export const NOT_PLAYABLE_HERE = "Several people play this game at one screen; it cannot be opened here.";

/** The first seat of a humans mask, 1-based. */
export const firstHumanSeat = (humans: number): number => 32 - Math.clz32(humans & -humans);

/** The seats of a humans mask, 1-based. */
export const humanSeatsOf = (humans: number, players: number): number[] =>
  Array.from({ length: players }, (_, i) => i + 1).filter((seat) => (humans >> (seat - 1)) & 1);

/**
 * A stored game in the shape the shell's list shows. The store keeps milliseconds; the shell
 * shows unix seconds. A game that several people play at one screen is listed but not played
 * here: the game screen shows one seat.
 */
export function offlineSummary(r: SavedGameSummary): OfflineGameSummary {
  const humanSeat = firstHumanSeat(r.summary.humans);
  const humanSeats = humanSeatsOf(r.summary.humans, r.summary.playerCount);
  return {
    gameId: r.id,
    playerCount: r.summary.playerCount,
    humanFaction: r.summary.factions[humanSeat - 1] ?? null,
    humanSeat,
    humanSeats,
    factions: r.summary.factions,
    playable: humanSeats.length === 1,
    round: r.summary.round,
    phase: r.summary.phase,
    answers: r.summary.answers,
    createdAt: Math.floor(r.createdAt / 1000),
    updatedAt: Math.floor(r.updatedAt / 1000),
    status: r.status,
    resumeEstimateMs: r.resumeEstimateMs,
  };
}

/** The list of the shell, newest first. */
export const listSummaries = async (store: OfflineSaveStore): Promise<OfflineGameSummary[]> =>
  (await store.list()).map(offlineSummary);

/** The status the shell's `OfflineStorage` shows, with the warning's text. */
export const shellStatus = (store: OfflineSaveStore) => {
  const status = store.status();
  return {
    persistence: status.persistence,
    warning: storageWarning(status),
    persisted: status.persisted,
    unreadable: status.unreadable,
  };
};
