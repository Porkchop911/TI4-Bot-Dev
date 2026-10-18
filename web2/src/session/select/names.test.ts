import { expect, it } from "vitest";
import type { Catalog } from "../wire";
import { commodityValue, factionName, forceOf, strategyCard, unitLabel, unitType } from "./names";

it("reads the type of a unit from its id, also of a faction and of an upgrade", () => {
  expect(unitType("carrier")).toBe("carrier");
  expect(unitType("sol_carrier")).toBe("carrier");
  expect(unitType("cruiser2")).toBe("cruiser");
  expect(unitType("sol_carrier2")).toBe("carrier");
  expect(unitType("fighter2")).toBe("fighter");
  expect(unitType("spacedock2")).toBe("dock");
  expect(unitType("warsun")).toBe("warsun");
  // The type is not always the last part of the id.
  expect(unitType("naaz_mech_space")).toBe("mech");
  expect(unitType("naalu_mech_te")).toBe("mech");
  expect(unitType("saar_spacedock2")).toBe("dock");
  expect(unitType("mentak_cruiser3")).toBe("cruiser");
  expect(unitType("token")).toBeNull();
});

it("names a faction and a strategy card from the catalogue, and by the id without it", () => {
  const catalog = {
    components: {
      faction_sheets: {
        jolnar: { name: "Jol-Nar", title: "The Universities of Jol-Nar", commodities: 4 },
      },
      strategy_cards: [{ id: "base4", name: "Construction", initiative: 4 }],
    },
  } as unknown as Catalog;
  expect(factionName("jolnar", catalog)).toBe("Jol-Nar");
  expect(commodityValue("jolnar", catalog)).toBe(4);
  expect(factionName("jolnar", null)).toBe("Jolnar");
  expect(commodityValue("jolnar", null)).toBeNull();
  // The id of a card of an older printing has no name in it.
  expect(strategyCard("base4", catalog)).toEqual({ number: 4, name: "Construction" });
  expect(strategyCard("base4", null)).toEqual({ number: 4, name: "base4" });
  expect(strategyCard("te6warfare", null)).toEqual({ number: 6, name: "Warfare" });
});

it("names a unit as its card prints it, and by its base unit without the catalogue", () => {
  const printed = {
    records: {
      units: [
        { id: "pds2", base_type: "pds", name: "PDS II" },
        { id: "titans_pds", base_type: "pds", name: "Hel-Titan I" },
        { id: "infantry", base_type: "infantry", name: "Infantry I" },
        { id: "infantry2", base_type: "infantry", name: "Infantry II" },
        { id: "sol_flagship", base_type: "flagship", name: "Genesis " },
      ],
    },
  } as unknown as Catalog;
  expect(unitLabel("pds2", printed)).toBe("PDS II");
  expect(unitLabel("titans_pds", printed)).toBe("Hel-Titan I");
  expect(unitLabel("sol_flagship", printed)).toBe("Genesis");
  // A unit that the catalogue does not have, and no catalogue: the base unit.
  expect(unitLabel("cruiser2", printed)).toBe("Cruiser");
  expect(unitLabel("pds2", null)).toBe("PDS");
  expect(unitLabel("fighter2", null, 2)).toBe("Fighters");
  // Counted by the printed unit: an Infantry II stays apart from an Infantry I.
  const units = [
    { unit_type: "infantry2", damaged: false },
    { unit_type: "infantry", damaged: false },
    { unit_type: "infantry2", damaged: true },
    { unit_type: "pds2", damaged: false },
  ];
  expect(forceOf(units, printed)).toEqual([
    { unit: "infantry", name: "Infantry I", count: 1, damaged: 0 },
    { unit: "infantry", name: "Infantry II", count: 2, damaged: 1 },
    { unit: "pds", name: "PDS II", count: 1, damaged: 0 },
  ]);
  expect(forceOf(units)).toEqual([
    { unit: "infantry", name: "Infantry", count: 3, damaged: 1 },
    { unit: "pds", name: "PDS", count: 1, damaged: 0 },
  ]);
});
