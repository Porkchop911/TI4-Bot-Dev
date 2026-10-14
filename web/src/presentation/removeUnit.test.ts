import { describe, expect, it } from "vitest";
import type { BoardView } from "../protocol/types.ts";
import { describeRemoveUnit, isRemoveUnitChoice } from "./removeUnit.ts";

const board = {
  systems: {
    "14": {
      system_id: "14",
      command_tokens: [],
      planets: {},
      units: [
        { unit_type: "carrier", owner: "a", planet: null, damaged: false },
        { unit_type: "carrier", owner: "a", planet: null, damaged: false },
        { unit_type: "fighter", owner: "a", planet: null, damaged: false },
        { unit_type: "infantry", owner: "a", planet: null, damaged: false },
        { unit_type: "infantry", owner: "a", planet: "mecatol", damaged: false },
        { unit_type: "cruiser", owner: "b", planet: null, damaged: false },
      ],
    },
  },
} as unknown as BoardView;

const supply = {
  prompt: "remove a unit: over fleet supply in 14",
  actor: "a",
  details: { fleet_limit: 1, fleet_charged: 4 },
};

describe("describeRemoveUnit", () => {
  it("ignores other decisions", () => {
    expect(isRemoveUnitChoice({ prompt: "pick a card" })).toBe(false);
    expect(describeRemoveUnit({ prompt: "pick a card", actor: "a" }, board)).toBeNull();
  });

  it("states the supply and the limit, with no raw ids", () => {
    const view = describeRemoveUnit(supply, board)!;
    expect(view.headline).toBe("3 over the fleet supply of 1 (4 ships counted) in system 14");
    expect(view.present).toBe("2 Carriers, 1 Fighter, 1 Infantry");
  });

  it("names the unit, where it is and what a carrier takes with it", () => {
    const view = describeRemoveUnit(supply, board)!;
    const carrier = view.option({ id: "remove|0", label: "remove sol_carrier2" });
    expect(carrier.title).toBe("Remove Carrier II");
    expect(carrier.where).toBe("In the space of system 14 (2 Carriers there)");
    expect(carrier.effect).toContain("1 Fighter, 1 Infantry");
    const cruiser = view.option({ id: "remove|1", label: "remove cruiser" });
    expect(cruiser.effect).toBe("Frees one place in the fleet supply.");
  });

  it("says how far over capacity the seat is", () => {
    const view = describeRemoveUnit(
      {
        prompt: "remove a unit: over capacity in 14",
        actor: "a",
        details: { capacity_consumed: 7, capacity_transport: 4 },
      },
      board,
    )!;
    expect(view.headline).toContain("3 over capacity in system 14");
    expect(view.option({ id: "remove|2", label: "remove fighter" }).effect).toBe(
      "Frees one place of capacity.",
    );
  });

  it("falls back to the board and the seat's tokens when the server sent no details", () => {
    const view = describeRemoveUnit(
      { prompt: "remove a unit: over fleet supply in 14", actor: "a" },
      board,
      { fleet_tokens: 1 },
    )!;
    expect(view.headline).toBe("1 over the fleet supply of 1 (2 ships counted) in system 14");
  });
});
