import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";

export const isPlaguePick = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  choice.context?.subtype === "plague_pick_planet";

export interface PlagueTarget {
  /** Infantry standing on the planet, whoever's they are (all of them roll). */
  infantry: number;
  /** Whose infantry they are: the seats that would lose units. */
  owners: string[];
  /** Plague kills on a 6 or better on a d10: 5 in 10. */
  chancePerDie: number;
  /** The kills to expect from the dice, one decimal. */
  expectedKills: number;
}

const isInfantry = (unitType: string) => /(^|_)infantry\d*$/.test(unitType);

/** What Plague would do on this planet: how many dice it rolls and what to expect, from the map the client holds. */
export function describePlagueTarget(
  choice: Pick<PendingChoiceDto, "context">,
  planetId: string,
  board?: BoardView | null,
): PlagueTarget | null {
  if (!isPlaguePick(choice) || !board) return null;
  let infantry = 0;
  const owners = new Set<string>();
  for (const system of Object.values(board.systems ?? {})) {
    for (const unit of system.units) {
      if (unit.planet === planetId && isInfantry(unit.unit_type)) {
        infantry += 1;
        owners.add(unit.owner);
      }
    }
  }
  const chancePerDie = 0.5;
  return {
    infantry,
    owners: [...owners],
    chancePerDie,
    expectedKills: Math.round(infantry * chancePerDie * 10) / 10,
  };
}
