import { expect, it } from "vitest";
import type { Catalog } from "../wire";
import { commodityValue, factionName, strategyCard, unitType } from "./names";

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
