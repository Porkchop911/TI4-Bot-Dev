import { describe, it, expect } from "vitest";
import { CONTENT, contentTables } from "./catalogStore.ts";
import type { ContentRecords } from "./types.ts";

const empty: ContentRecords = {
  strategy_cards: [],
  secret_objectives: [],
  public_objectives: [],
  action_cards: [],
  technologies: [],
  explores: [],
  planets: [],
  attachments: [],
  factions: [],
  units: [],
  leaders: [],
  abilities: [],
  promissory_notes: [],
  relics: [],
  breakthroughs: [],
};

describe("the tables of the catalog", () => {
  it("are filled from the catalog of the engine before a test runs", () => {
    expect(CONTENT.actionCards.sabo1?.name).toBe("Sabotage");
    expect(CONTENT.units.sol_carrier?.productionValue).toBeUndefined();
    expect(CONTENT.units.spacedock?.productionValue).toBe("+2");
  });

  it("keep each kind by id, in the order of the ids, the first of two records with one id", () => {
    const tables = contentTables({
      ...empty,
      relics: [
        { id: "b", name: "B" },
        { id: "a", name: "A" },
        { id: "b", name: "B again" },
      ],
    });
    expect(Object.keys(tables.relics)).toEqual(["a", "b"]);
    expect(tables.relics.b.name).toBe("B");
  });

  it("drop the printed window of a plain action card and add the automation id", () => {
    const tables = contentTables({
      ...empty,
      action_cards: [
        { id: "dh1", name: "Direct Hit", window: "After another player's ship uses SUSTAIN DAMAGE", text: "Destroy that ship.", automation_id: "direct_hit" },
        { id: "plague", name: "Plague", phase: "Action", window: "Action", text: "Choose 1 planet." },
      ],
    });
    expect(tables.actionCards.direct_hit).toEqual({ ...tables.actionCards.dh1, id: "direct_hit" });
    expect(tables.actionCards.plague).toEqual({ id: "plague", name: "Plague", phase: "Action", description: "Choose 1 planet." });
  });

  it("give a planet without specialties an empty list", () => {
    const tables = contentTables({
      ...empty,
      planets: [{ id: "x", name: "X", resources: 1, influence: 2 }],
    });
    expect(tables.planets.x).toEqual({ id: "x", name: "X", resources: 1, influence: 2, techSpecialties: [] });
  });
});
