import { isPublicEvent } from "../protocol/eventVisibility.ts";
import type { BoardView, GameEvent, PendingChoiceDto, TableView } from "../protocol/types.ts";
import { planetName } from "./invasionSteps.ts";
import { scoringObjectiveMeta } from "./scoringOptions.ts";

/** One home planet a seat does not control, and who holds it (null: nobody). */
export interface LostHomePlanet {
  planet: string;
  name: string;
  holder: string | null;
}

/** Who controls a planet on the board, or null. */
export function planetHolder(board: BoardView | null | undefined, planet: string): string | null {
  for (const system of Object.values(board?.systems ?? {})) {
    const view = system.planets?.[planet];
    if (view) return view.controlled_by ?? null;
  }
  return null;
}

/**
 * Rule 61.16: the home planets that bar `seat` from scoring public objectives, from the table's
 * `home_system_lost` (the engine's own test, so Saar's Nomadic is already honoured). Empty when
 * nothing bars them.
 */
export function lostHomePlanets(
  table: Pick<TableView, "home_system_lost"> | null | undefined,
  board: BoardView | null | undefined,
  seat: string | null | undefined,
): LostHomePlanet[] {
  if (!seat) return [];
  return (table?.home_system_lost?.[seat] ?? []).map((planet) => ({
    planet,
    name: planetName(planet),
    holder: planetHolder(board, planet),
  }));
}

/** "Archon Ren (held by Letnev)", joined with "and". */
export function lostHomeText(lost: readonly LostHomePlanet[], label: (seat: string) => string): string {
  const parts = lost.map((p) => (p.holder ? `${p.name} (held by ${label(p.holder)})` : p.name));
  if (parts.length <= 1) return parts.join("");
  return `${parts.slice(0, -1).join(", ")} and ${parts[parts.length - 1]}`;
}

/** The banner sentence: why no public objective can be scored. */
export function homeBlockSentence(lost: readonly LostHomePlanet[], label: (seat: string) => string): string {
  return `You cannot score public objectives: you do not control ${lostHomeText(lost, label)} in your home system (rule 61.16).`;
}

const PLAYED_IMPERIAL = "played Imperial";

/** The viewer's own "played Imperial" log entry among `fresh`, if any. */
export function ownImperialPlay(fresh: readonly GameEvent[], viewerSeat: string | null | undefined): GameEvent | null {
  if (!viewerSeat) return null;
  return (
    fresh.find(
      (e) =>
        isPublicEvent(e) &&
        e.actor === viewerSeat &&
        e.detail?.trim() === `${viewerSeat} ${PLAYED_IMPERIAL}`,
    ) ?? null
  );
}

/**
 * Why Imperial scored nothing for the viewer, or null when it may have (or we cannot tell).
 *
 * The engine skips Imperial's scoring question entirely when nothing is scoreable, so without
 * this the play looks like it was swallowed. Two cases are certain from the view alone:
 * - a home planet is lost (61.16): nothing public can be scored at all;
 * - every unscored revealed objective has progress and none is met.
 * While the viewer is being asked which objective to score, nothing is said.
 */
export function imperialNoScoreNotice(input: {
  viewerSeat: string;
  table: Pick<TableView, "home_system_lost" | "revealed_objectives" | "scored_objectives" | "objective_progress"> | null | undefined;
  board: BoardView | null | undefined;
  pendingChoice?: PendingChoiceDto | null;
  label: (seat: string) => string;
}): string | null {
  const { viewerSeat, table, board, pendingChoice, label } = input;
  if (!table) return null;
  if (pendingChoice?.actor === viewerSeat && pendingChoice.context?.subtype === "imperial_score_objective")
    return null;
  const lost = lostHomePlanets(table, board, viewerSeat);
  if (lost.length > 0)
    return `No public objective could be scored: you do not control ${lostHomeText(lost, label)} in your home system (rule 61.16).`;
  const scored = new Set(table.scored_objectives?.[viewerSeat] ?? []);
  const open = table.revealed_objectives.filter((id) => !scored.has(id));
  if (open.length === 0) return "No public objective could be scored: you have scored every revealed one.";
  const progress = table.objective_progress?.[viewerSeat] ?? {};
  if (open.some((id) => !progress[id] || progress[id].satisfied)) return null;
  const rows = open.map((id) => `${scoringObjectiveMeta(id).name} ${progress[id].have}/${progress[id].threshold}`);
  return `No public objective could be scored: none is met (${rows.join(", ")}).`;
}
