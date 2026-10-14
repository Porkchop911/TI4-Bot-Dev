import { describe, expect, it } from "vitest";
import type { BoardView } from "../protocol/types.ts";
import { describeUnitAbilityOption } from "./unitAbilityOptions.ts";

const board = {
  systems: {
    "20": {
      system_id: "20",
      command_tokens: [],
      planets: { hercant: { planet_id: "hercant", controlled_by: "a", exhausted: false } },
      units: [{ unit_type: "infantry", owner: "a", planet: "hercant", damaged: false }],
    },
  },
} as unknown as BoardView;

describe("describeUnitAbilityOption", () => {
  it("names a Transit Diodes move with both ends and who is already there", () => {
    const info = describeUnitAbilityOption(
      { prompt: "Transit Diodes" },
      {
        id: "transit|18|arretze|hacan_mech|20|hercant",
        label: "move hacan_mech from arretze to hercant",
        kind: "transit",
        payload: {
          source_system: "18",
          source: "arretze",
          unit: "hacan_mech",
          destination_system: "20",
          planet: "hercant",
        },
      },
      board,
    )!;
    expect(info.title).toBe("Move Mech");
    expect(info.lines[0]).toBe("From Arretze in Mecatol Rex (#18)");
    expect(info.lines[1]).toBe("To Hercant in System #20");
    expect(info.lines[2]).toBe("Already there: 1 infantry");
    expect(info.systems).toEqual(["18", "20"]);
    expect(JSON.stringify([info.title, info.lines])).not.toContain("hacan_mech");
  });

  it("names an Orbital Drop deploy with its cost", () => {
    const info = describeUnitAbilityOption(
      { prompt: "Orbital Drop: deploy a mech on hercant" },
      {
        id: "deploy|sol_mech|1",
        label: "deploy 1 sol_mech for 3 resources",
        payload: { unit: "sol_mech", count: 1, cost: 3, system: "20", planet: "hercant", orbital_drop_deploy: true },
      },
      board,
    )!;
    expect(info.title).toBe("Deploy Mech");
    expect(info.lines).toContain("Cost: 3 resources");
    expect(info.lines[0]).toMatch(/^On .* in System #20$/);
  });

  it("names a Sling Relay production and leaves other builds alone", () => {
    const opt = {
      id: "build|sol_carrier2|1",
      label: "produce 1x sol_carrier2 for 3",
      payload: { unit: "sol_carrier2", count: 1, cost: 3, system: "20" },
    };
    const info = describeUnitAbilityOption({ prompt: "produce one unit in 20" }, opt, board)!;
    expect(info.title).toBe("Produce Carrier II");
    expect(info.lines).toEqual(["In the space area of System #20", "Cost: 3"]);
    expect(describeUnitAbilityOption({ prompt: "produce in 20" }, opt, board)).toBeNull();
  });

  it("ignores unrelated options", () => {
    expect(describeUnitAbilityOption({ prompt: "x" }, { id: "a", label: "a" })).toBeNull();
  });
});
