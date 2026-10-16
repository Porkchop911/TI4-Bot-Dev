// The odds of a combat from an answer of `ti4-calc`: Win, Draw and Loss of the attacker, and the
// survivors of each side by kind against the units that fight. The space odds of a movement and
// the ground odds of the landings share them.
import type { CombatOddsView, UnitType } from "../../model";
import type { Catalog } from "../wire";
import { byUnitOrder, unitKey, unitLabel, unitType } from "./names";

/** What both answers of `ti4-calc` have (`BattleResponse`, `GroundOddsResponse`). */
export interface CombatAnswer {
  simulations: number;
  attacker_win_rate: number;
  defender_win_rate: number;
  mutual_destruction_rate: number;
  attacker_expected_survivors: Record<string, number>;
  defender_expected_survivors: Record<string, number>;
  attacker_fielded: Record<string, number>;
  defender_fielded: Record<string, number>;
}

/**
 * Whole percentages of `rates` (fractions that sum to 1) that sum to 100: the rates that rounding
 * down cuts most get the points that are left.
 */
export function percentages(rates: number[]): number[] {
  const exact = rates.map((rate) => Math.max(0, rate) * 100);
  const whole = exact.map(Math.floor);
  const order = exact
    .map((value, index) => ({ index, cut: value - whole[index] }))
    .sort((a, b) => b.cut - a.cut || a.index - b.index);
  let left = 100 - whole.reduce((sum, value) => sum + value, 0);
  for (const { index } of order) {
    if (left <= 0) {
      break;
    }
    whole[index] += 1;
    left -= 1;
  }
  return whole;
}

/**
 * Counts by unit id, as counts by the printed unit (`unitKey`), of the kinds that `keep` takes.
 * Each has one of its ids.
 */
export function byUnit(
  counts: Record<string, number> | undefined,
  printed: Catalog | null,
  keep: (kind: UnitType) => boolean = () => true,
): Map<string, { id: string; count: number }> {
  const units = new Map<string, { id: string; count: number }>();
  for (const [id, count] of Object.entries(counts ?? {})) {
    const kind = unitType(id);
    if (kind && keep(kind) && count > 0) {
      const key = unitKey(id, printed);
      units.set(key, { id, count: (units.get(key)?.count ?? 0) + count });
    }
  }
  return units;
}

/**
 * The odds of a combat. `response` is the last answer (null before the first); while it is of
 * other sides (`pending`), `fielded` gives the units that fight now. `draw` is the share of the
 * rollouts that is neither a win nor a loss.
 */
export function combatOdds({
  attacker,
  defender,
  response,
  draw,
  pending,
  fielded,
  keep,
  printed = null,
}: {
  attacker: string;
  defender: string;
  response: CombatAnswer | null;
  draw: number;
  pending: boolean;
  /** The units of each side by unit id, of the staged units. */
  fielded: {
    attacker: Record<string, number> | undefined;
    defender: Record<string, number> | undefined;
  };
  /** The kinds that fight. */
  keep?: (kind: UnitType) => boolean;
  /** The printed names of the units. */
  printed?: Catalog | null;
}): CombatOddsView {
  const [win, even, loss] = response
    ? percentages([response.attacker_win_rate, draw, response.defender_win_rate])
    : [null, null, null];
  const side = (
    seat: string,
    units: Record<string, number> | undefined,
    expected: Record<string, number> | undefined,
  ) => {
    const fights = byUnit(units, printed, keep);
    const survives = byUnit(expected, printed);
    return {
      seat,
      units: [...fights]
        .sort(([, a], [, b]) => byUnitOrder(a.id, b.id))
        .map(([key, { id, count }]) => ({
          unit: unitType(id)!,
          name: unitLabel(id, printed, count),
          expected: response ? (survives.get(key)?.count ?? 0).toFixed(1) : null,
          fielded: count,
        })),
    };
  };
  return {
    defender,
    win,
    draw: even,
    loss,
    rollouts: response?.simulations ?? 0,
    pending,
    sides: [
      side(
        attacker,
        pending ? fielded.attacker : response?.attacker_fielded,
        response?.attacker_expected_survivors,
      ),
      side(
        defender,
        pending ? fielded.defender : response?.defender_fielded,
        response?.defender_expected_survivors,
      ),
    ],
  };
}
