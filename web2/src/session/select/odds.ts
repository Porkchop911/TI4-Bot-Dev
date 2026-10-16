// The odds of a combat from an answer of `ti4-calc`: Win, Draw and Loss of the attacker, and the
// survivors of each side by kind against the units that fight. The space odds of a movement and
// the ground odds of the landings share them.
import type { CombatOddsView, UnitType } from "../../model";
import { UNIT_ORDER, unitType } from "./names";

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

/** Counts by unit id, as counts by kind, of the kinds that `keep` takes. */
export function byKind(
  counts: Record<string, number> | undefined,
  keep: (kind: UnitType) => boolean = () => true,
): Map<UnitType, number> {
  const kinds = new Map<UnitType, number>();
  for (const [unit, count] of Object.entries(counts ?? {})) {
    const kind = unitType(unit);
    if (kind && keep(kind) && count > 0) {
      kinds.set(kind, (kinds.get(kind) ?? 0) + count);
    }
  }
  return kinds;
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
}): CombatOddsView {
  const [win, even, loss] = response
    ? percentages([response.attacker_win_rate, draw, response.defender_win_rate])
    : [null, null, null];
  const side = (
    seat: string,
    units: Record<string, number> | undefined,
    expected: Record<string, number> | undefined,
  ) => {
    const fights = byKind(units, keep);
    const survives = byKind(expected);
    return {
      seat,
      units: UNIT_ORDER.filter((unit) => fights.has(unit)).map((unit) => ({
        unit,
        expected: response ? (survives.get(unit) ?? 0).toFixed(1) : null,
        fielded: fights.get(unit) ?? 0,
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
