import type { InvasionStepView, PlacedUnitView } from "../protocol/types.ts";
import { countBy, type UnitCount } from "./combatSummary.ts";

/** Structures sit on a planet but take no part in ground combat. */
const NON_COMBATANTS = new Set(["pds", "space_dock", "spacedock"]);

export interface GroundSideSummary {
  seat: string;
  /** Ground forces on the planet at the start of the summarised round. */
  before: UnitCount[];
  /** Ground forces left on the planet. */
  after: UnitCount[];
  lost: UnitCount[];
  sustained: UnitCount[];
  remaining: number;
  /** Hits this side scored in the round. */
  hits: number | null;
}

export type GroundVerdict =
  | { kind: "attacker_won"; winner: string; loser: string }
  | { kind: "defender_held"; winner: string; loser: string }
  | { kind: "mutual_destruction" };

export interface GroundCombatSummary {
  planet: string;
  round: number;
  attacker: GroundSideSummary;
  defender: GroundSideSummary;
  verdict: GroundVerdict;
}

const toList = (counts: Map<string, number>): UnitCount[] =>
  [...counts].map(([unit, count]) => ({ unit, count }));

const fighters = (units: readonly PlacedUnitView[], seat: string) =>
  units.filter((u) => u.owner === seat && !NON_COMBATANTS.has(u.unit_type.toLowerCase()));

function side(
  seat: string,
  start: readonly PlacedUnitView[],
  now: readonly PlacedUnitView[],
  hits: number | null,
): GroundSideSummary {
  const before = fighters(start, seat);
  const after = fighters(now, seat);
  const beforeCounts = countBy(before);
  const afterCounts = countBy(after);
  const beforeDamaged = countBy(before.filter((u) => u.damaged));
  const afterDamaged = countBy(after.filter((u) => u.damaged));
  const lost: UnitCount[] = [];
  const sustained: UnitCount[] = [];
  for (const [unit, count] of beforeCounts) {
    const gone = count - (afterCounts.get(unit) ?? 0);
    if (gone > 0) lost.push({ unit, count: gone });
  }
  for (const [unit, count] of afterDamaged) {
    const fresh = Math.min(count - (beforeDamaged.get(unit) ?? 0), afterCounts.get(unit) ?? 0);
    if (fresh > 0) sustained.push({ unit, count: fresh });
  }
  return {
    seat,
    before: toList(beforeCounts),
    after: toList(afterCounts),
    lost,
    sustained,
    remaining: after.length,
    hits,
  };
}

/**
 * The closing summary of a ground combat: the final round's `step` as the server sends it, seen
 * from the invader's side. Returns null while the fight goes on (ground forces of both sides are
 * left), when the step is not a ground round, or when no opposing side can be identified.
 * `before`/`after` cover the whole planet at the start and end of that last round only.
 */
export function summarizeGroundCombat(
  step: InvasionStepView | null | undefined,
  invader: string,
): GroundCombatSummary | null {
  if (!step || step.kind !== "ground_round") return null;
  const owners = [
    ...Object.keys(step.hits),
    ...step.before.map((u) => u.owner),
    ...step.after.map((u) => u.owner),
  ];
  const defenderSeat = owners.find((o) => o !== invader);
  if (!defenderSeat) return null;
  const attacker = side(invader, step.before, step.after, step.hits[invader] ?? null);
  const defender = side(defenderSeat, step.before, step.after, step.hits[defenderSeat] ?? null);
  let verdict: GroundVerdict;
  if (attacker.remaining === 0 && defender.remaining === 0) verdict = { kind: "mutual_destruction" };
  else if (defender.remaining === 0) verdict = { kind: "attacker_won", winner: invader, loser: defenderSeat };
  else if (attacker.remaining === 0) verdict = { kind: "defender_held", winner: defenderSeat, loser: invader };
  else return null;
  return { planet: step.planet, round: step.round, attacker, defender, verdict };
}
