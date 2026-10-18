import { describe, expect, it } from "vitest";
import type { PlanetView } from "../wire/index.ts";
import {
  applyBoosts,
  boostsFor,
  planetValuesNow,
  techSpecialtiesNow,
  wireBoosts,
} from "./values.ts";

const MINING = { source: "Mining World", resources: 2, influence: 0 };
const DYSON = { source: "Dyson Sphere", resources: 2, influence: 1 };
const LODOR = { resources: 3, influence: 1 };

function view(extra: Partial<PlanetView>): PlanetView {
  return { planet_id: "lodor", controlled_by: "a", exhausted: false, ...extra };
}

describe("planet values now", () => {
  it("adds every boost to the printed values", () => {
    expect(applyBoosts(3, 1, [MINING, DYSON])).toEqual({ resources: 7, influence: 2 });
    expect(boostsFor([MINING, DYSON], "influence")).toEqual([
      { source: "Dyson Sphere", amount: 1 },
    ]);
    expect(wireBoosts(view({ value_boosts: [MINING] }))).toEqual([MINING]);
    expect(wireBoosts(undefined)).toEqual([]);
  });

  it("prefers the engine's totals", () => {
    expect(planetValuesNow(LODOR, view({ resources: 9, value_boosts: [MINING] }))).toEqual({
      resources: 9,
      influence: 1,
    });
  });

  it("falls back to printed plus boosts, and to printed alone", () => {
    expect(planetValuesNow(LODOR, view({ value_boosts: [MINING, DYSON] }))).toEqual({
      resources: 7,
      influence: 2,
    });
    expect(planetValuesNow(LODOR, view({}))).toEqual(LODOR);
    expect(planetValuesNow(LODOR, undefined)).toEqual(LODOR);
  });

  it("gives the same values from the totals as from printed plus boosts", () => {
    const boosts = [MINING, DYSON];
    const totals = applyBoosts(LODOR.resources, LODOR.influence, boosts);
    const withTotals = planetValuesNow(LODOR, view({ value_boosts: boosts, ...totals }));
    const withBoosts = planetValuesNow(LODOR, view({ value_boosts: boosts }));
    expect(withTotals).toEqual(withBoosts);
  });

  it("takes the engine's tech specialties, else the printed ones", () => {
    expect(techSpecialtiesNow(["biotic"], view({ tech_specialties: ["warfare"] }))).toEqual([
      "warfare",
    ]);
    expect(techSpecialtiesNow(["biotic"], view({}))).toEqual(["biotic"]);
    expect(techSpecialtiesNow(undefined, null)).toEqual([]);
  });
});
