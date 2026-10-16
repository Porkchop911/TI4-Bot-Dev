import { describe, expect, it } from "vitest";
import { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import {
  describeSystem,
  isSystemPickChoice,
  planetOccupants,
  summarizeUnits,
  systemTag,
} from "./systemFacts.ts";
import { buildBoardPresentationModel } from "./boardPresentation.ts";
import { resolveMapTargetSelection } from "./planetSelection.ts";
import { partialContext } from "../test/wire.ts";

const board: BoardView = {
  systems: {
    "14": {
      system_id: "14",
      command_tokens: ["p2"],
      units: [
        { unit_type: "sol_carrier2", owner: "p1", damaged: false, planet: null, galvanized: false },
        { unit_type: "fighter", owner: "p1", damaged: false, planet: null, galvanized: false },
        { unit_type: "fighter", owner: "p1", damaged: false, planet: null, galvanized: false },
        { unit_type: "infantry", owner: "p1", planet: "arinam", damaged: false, galvanized: false },
      ],
      planets: { arinam: { planet_id: "arinam", controlled_by: "p1", exhausted: false } },
    },
    "18": { system_id: "18", command_tokens: [], units: [], planets: {} },
  },
  map_tiles: [
    {
      system_id: "14",
      label: "Arinam",
      q: 0,
      r: 0,
      planets: [{ id: "arinam", label: "Arinam", resources: 1, influence: 2 }],
    },
    { system_id: "18", label: "Mecatol Rex", q: 1, r: 0, planets: [] },
  ],
};

const diplomacy: PendingChoiceDto = {
  nonce: "d1",
  actor: "p1",
  prompt: "choose a system",
  context: partialContext({ subtype: "diplomacy_choose_system", source: { StrategyCard: { card: "Diplomacy", secondary: false } } }),
  options: [
    { id: "14", kind: "system", label: "14", payload: {} },
    { id: "18", kind: "system", label: "18", payload: {} },
  ],
};

describe("system facts", () => {
  it("summarizes planets, ships and tokens of a system", () => {
    const facts = describeSystem("14", board);
    expect(facts.title).toBe("Arinam (#14)");
    expect(facts.planets[0]).toMatchObject({ name: "Arinam", resources: 1, influence: 2 });
    expect(facts.ships).toEqual([{ owner: "p1", summary: "1 carrier, 2 fighters" }]);
    expect(facts.commandTokens).toEqual(["p2"]);
    expect(describeSystem("18", board).title).toBe("Mecatol Rex (#18)");
  });

  it("names units and groups planet occupants", () => {
    expect(summarizeUnits([{ unit_type: "pds" }, { unit_type: "pds" }])).toBe("2 PDS");
    expect(planetOccupants("14", "arinam", board)).toEqual([{ owner: "p1", summary: "1 infantry" }]);
  });

  it("recognises a bare system pick, only when every option is a board system", () => {
    expect(isSystemPickChoice(diplomacy, board)).toBe(true);
    expect(isSystemPickChoice(diplomacy, null)).toBe(false);
    const bogus = { ...diplomacy, options: [{ id: "999", kind: "system", label: "999", payload: {} }] };
    expect(isSystemPickChoice(bogus, board)).toBe(false);
    const other = { ...diplomacy, context: partialContext({ subtype: "gain_command_token" }) };
    expect(isSystemPickChoice(other, board)).toBe(false);
    // Unexpected Action: recall your own token from one of the systems that hold it.
    const unexpected = { ...diplomacy, context: partialContext({ subtype: "unexpected_pick_recall" }) };
    expect(isSystemPickChoice(unexpected, board)).toBe(true);
  });

  it("highlights the candidate systems and a map click selects the same option id", () => {
    const model = buildBoardPresentationModel(board, ["p1", "p2"], [], diplomacy, "p1");
    expect(model.tiles.filter((t) => t.isCandidateTarget).map((t) => t.systemId).sort()).toEqual([
      "14",
      "18",
    ]);
    expect(resolveMapTargetSelection(diplomacy, "14", undefined, board)).toEqual({
      kind: "select",
      optionId: "14",
      planetId: null,
    });
  });
});

describe("systemTag", () => {
  const tile = (system_id: string, label: string, planets: string[] = [], anomalies: string[] = []) => ({
    system_id,
    label,
    q: 0,
    r: 0,
    anomalies,
    planets: planets.map((id) => ({ id, label: id, resources: 1, influence: 1, traits: [], tech_specialties: [] })),
  });
  const tagged = {
    systems: {},
    map_tiles: [
      tile("18", "Mecatol Rex", ["mr"]),
      tile("01", "Jord - Sol", ["jord"]),
      tile("10", "Arc Prime/Wren Terra - Letnev", ["arcprime", "wrenterra"]),
      tile("17", "Creuss Gate - Creuss"),
      tile("82b", "Mallice - Nexus", ["mallice"]),
      tile("41", "Gravity Rift", [], ["gravity rift"]),
      tile("46", "Empty System"),
      tile("67", "Cormund", ["cormund"], ["gravity rift"]),
      tile("27", "Starpoint/New Albion", ["starpoint", "newalbion"]),
    ],
  } as unknown as BoardView;

  it("names planet systems, home systems and anomalies with their tile number", () => {
    expect(systemTag("18", tagged)).toBe("#18 [Mecatol Rex]");
    expect(systemTag("01", tagged)).toBe("#01 [Sol home system]");
    expect(systemTag("10", tagged)).toBe("#10 [Letnev home system]");
    expect(systemTag("17", tagged)).toBe("#17 [Creuss Gate]");
    expect(systemTag("82b", tagged)).toBe("#82b [Mallice]");
    expect(systemTag("41", tagged)).toBe("#41 [Gravity rift]");
    expect(systemTag("46", tagged)).toBe("#46 [Empty system]");
    expect(systemTag("67", tagged)).toBe("#67 [Cormund, gravity rift]");
    expect(systemTag("27", tagged)).toBe("#27 [Starpoint/New Albion]");
  });

  it("falls back to the bare tile number when the board does not know the tile", () => {
    expect(systemTag("5", null)).toBe("#05");
    expect(systemTag("99", tagged)).toBe("#99");
    expect(systemTag("18", null)).toBe("#18 [Mecatol Rex]");
  });
});
