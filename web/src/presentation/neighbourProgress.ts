import type { ObjectiveProgressView } from "../protocol/types.ts";
import type { ParticipantDisplay } from "./playerDisplay.ts";

export type NeighbourStanding = "ahead" | "tied" | "behind";

export interface NeighbourRowDisplay {
  player: string;
  label: string;
  color: string;
  symbol: string;
  /** Seat position, shown in the seat colour so the neighbour reads without colour too. */
  position: number | null;
  mine: number;
  theirs: number;
  standing: NeighbourStanding;
  /** "12/10": our planets first, theirs second. */
  text: string;
  /** "12 planets against Alice's 10: ahead". */
  aria: string;
}

export interface NeighbourProgressDisplay {
  rows: NeighbourRowDisplay[];
  ahead: number;
  threshold: number;
  /** "Ahead of 1 of 3 neighbors · need 2". */
  summary: string;
}

const STANDING_WORD: Record<NeighbourStanding, string> = {
  ahead: "ahead",
  tied: "tied, not ahead",
  behind: "behind",
};

/**
 * Push Boundaries' per-neighbour comparison, worded. The server's `ahead` flag decides the
 * standing (the engine's own "more planets than" test), so a tie is never shown as ahead.
 * Null when the progress carries no neighbour rows (any other objective, or no map).
 */
export function neighbourProgressDisplay(
  progress: ObjectiveProgressView | undefined,
  display: (id: string) => ParticipantDisplay,
): NeighbourProgressDisplay | null {
  const neighbours = progress?.neighbours;
  if (!progress || !neighbours || neighbours.length === 0) return null;
  const rows = neighbours.map((row): NeighbourRowDisplay => {
    const who = display(row.player);
    const standing: NeighbourStanding = row.ahead
      ? "ahead"
      : row.mine === row.theirs
        ? "tied"
        : "behind";
    return {
      player: row.player,
      label: who.label,
      color: who.color,
      symbol: who.symbol,
      position: who.position,
      mine: row.mine,
      theirs: row.theirs,
      standing,
      text: `${row.mine}/${row.theirs}`,
      aria: `${row.mine} planets against ${who.label}'s ${row.theirs}: ${STANDING_WORD[standing]}`,
    };
  });
  const ahead = rows.filter((row) => row.standing === "ahead").length;
  const noun = rows.length === 1 ? "neighbor" : "neighbors";
  return {
    rows,
    ahead,
    threshold: progress.threshold,
    summary: `Ahead of ${ahead} of ${rows.length} ${noun} · need ${progress.threshold}`,
  };
}
