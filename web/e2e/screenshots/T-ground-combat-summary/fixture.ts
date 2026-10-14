import type { BoardView, PlacedUnitView } from "../../../src/protocol/types";
import { actor, galleryBoard } from "../_shared/fixtures";

const RIVAL = "other_seat";
const on = (unit_type: string, owner: string, damaged = false): PlacedUnitView => ({ unit_type, owner, planet: "mecatol_rex", damaged });
const many = (n: number, type: string, owner: string) => Array.from({ length: n }, () => on(type, owner));

const start = [...many(4, "infantry", actor), on("mech", actor), ...many(3, "infantry", RIVAL), on("pds", RIVAL)];

/** Planet-level forces left after the final round, per outcome. */
export const outcomes = {
  attackerWins: { after: [...many(2, "infantry", actor), on("mech", actor, true), on("pds", RIVAL)], hits: { [actor]: 4, [RIVAL]: 2 } },
  defenderHolds: { after: [on("infantry", RIVAL), on("pds", RIVAL)], hits: { [actor]: 1, [RIVAL]: 5 } },
  mutualDestruction: { after: [on("pds", RIVAL)], hits: { [actor]: 5, [RIVAL]: 3 } },
};

/** The game board right after the last ground round on Mecatol Rex (synthetic, not an engine capture). */
export function groundCombatBoard(outcome: (typeof outcomes)[keyof typeof outcomes]): BoardView {
  const system = galleryBoard.systems["18"];
  return {
    ...galleryBoard,
    combat: null,
    invasion: {
      system_id: "18",
      invasion_seq: 3,
      invader: actor,
      phase: "planet_result",
      planets: ["mecatol_rex"],
      current_planet: "mecatol_rex",
      defender: null,
      ground_round: 3,
      last_step: {
        planet: "mecatol_rex",
        kind: "ground_round",
        round: 3,
        before: start,
        after: outcome.after,
        dice: [],
        hits: outcome.hits,
        harrow_hits: 0,
      },
    },
    systems: { ...galleryBoard.systems, "18": { ...system, units: outcome.after } },
  };
}
