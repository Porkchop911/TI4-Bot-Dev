import type { CombatView, PlacedUnitView } from "../protocol/types.ts";

export interface UnitCount {
  unit: string;
  count: number;
}

export interface CombatSideSummary {
  seat: string;
  /** Ships destroyed during the summarised round. */
  lost: UnitCount[];
  /** Ships that sustained damage and survived. */
  sustained: UnitCount[];
  remaining: number;
  /** Hits this side scored, when the server reports them. */
  hits: number | null;
}

export type CombatVerdict =
  | { kind: "won"; winner: string; loser: string }
  | { kind: "annihilated" }
  | { kind: "both_remain" };

export interface CombatSummary {
  round: number;
  attacker: CombatSideSummary;
  defender: CombatSideSummary;
  verdict: CombatVerdict;
}

const spaceUnits = (units: readonly PlacedUnitView[], seat: string) =>
  units.filter((u) => u.owner === seat && !u.planet);

export function countBy(units: readonly PlacedUnitView[]) {
  const counts = new Map<string, number>();
  for (const u of units) {
    const key = u.unit_type.toLowerCase();
    counts.set(key, (counts.get(key) ?? 0) + 1);
  }
  return counts;
}

function side(
  seat: string,
  start: readonly PlacedUnitView[],
  now: readonly PlacedUnitView[],
  hits: number | null,
): CombatSideSummary {
  const before = spaceUnits(start, seat);
  const after = spaceUnits(now, seat);
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
  return { seat, lost, sustained, remaining: after.length, hits };
}

/**
 * What the last round of a space combat cost each side, and who is left standing. `round_start`
 * is the fleets at the start of the round, so earlier rounds are not part of the summary.
 * Returns null when the server has not sent the starting fleets.
 */
export function summarizeCombat(
  combat: CombatView,
  systemUnits: readonly PlacedUnitView[],
): CombatSummary | null {
  const start = combat.round_start;
  if (!start) return null;
  const attacker = side(combat.attacker, start, systemUnits, combat.attacker_hits ?? null);
  const defender = side(combat.defender, start, systemUnits, combat.defender_hits ?? null);
  const verdict: CombatVerdict =
    attacker.remaining === 0 && defender.remaining === 0
      ? { kind: "annihilated" }
      : defender.remaining === 0
        ? { kind: "won", winner: combat.attacker, loser: combat.defender }
        : attacker.remaining === 0
          ? { kind: "won", winner: combat.defender, loser: combat.attacker }
          : { kind: "both_remain" };
  return { round: combat.round, attacker, defender, verdict };
}
