import { describe, expect, it } from "vitest";
import { describePlagueTarget, isPlaguePick } from "./plagueTarget.ts";
import type { BoardView } from "../protocol/types.ts";

const choice = { context: { subtype: "plague_pick_planet" } as never };
const unit = (unit_type: string, owner: string, planet: string | null) => ({ unit_type, owner, planet, damaged: false });
const board = {
  systems: {
    "14": {
      system_id: "14",
      command_tokens: [],
      planets: {},
      units: [unit("infantry", "b", "archonren"), unit("infantry", "b", "archonren"), unit("infantry2", "b", "archonren"), unit("mech", "b", "archonren"), unit("infantry", "c", "archontau"), unit("cruiser", "b", null)],
    },
  },
} as unknown as BoardView;

describe("Plague's target facts", () => {
  it("recognises the card's planet pick", () => {
    expect(isPlaguePick(choice)).toBe(true);
    expect(isPlaguePick({ context: { subtype: "mining_initiative_pick_planet" } as never })).toBe(false);
  });

  it("counts the infantry that would roll, whose they are and how many to expect to die", () => {
    expect(describePlagueTarget(choice, "archonren", board)).toEqual({ infantry: 3, owners: ["b"], chancePerDie: 0.5, expectedKills: 1.5 });
    expect(describePlagueTarget(choice, "archontau", board)?.infantry).toBe(1);
  });

  it("says no infantry when the planet holds none, and nothing without a board", () => {
    expect(describePlagueTarget(choice, "nowhere", board)?.infantry).toBe(0);
    expect(describePlagueTarget(choice, "archonren", null)).toBeNull();
  });
});
