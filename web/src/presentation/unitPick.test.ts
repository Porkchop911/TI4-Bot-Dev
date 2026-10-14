import { describe, expect, it } from "vitest";
import { describeUnitPickCard, describeUnitPickOption, isUnitPick } from "./unitPick.ts";

const units = {
  "26|0": { system: "26", planet: null, unit: "dreadnought2", damaged: true, cost: 4 },
  "27|1": { system: "27", planet: null, unit: "cruiser", damaged: false, cost: 2 },
};
const refitUnits = { "14|arinam|0": { system: "14", planet: "arinam", unit: "infantry", damaged: false, cost: 0.5 } };
const scuttle = {
  context: { subtype: "scuttle_pick_ship", source: { ActionCard: "scuttle" } } as never,
  details: { units },
};
const refit = {
  context: { subtype: "refit_pick_infantry", source: { ActionCard: "refit" } } as never,
  details: { units: refitUnits },
};

describe("unit picks (Refit Troops, Scuttle)", () => {
  it("recognises the two decisions", () => {
    expect(isUnitPick(scuttle)).toBe(true);
    expect(isUnitPick(refit)).toBe(true);
    expect(isUnitPick({ context: { subtype: "spy_pick_player" } as never })).toBe(false);
  });

  it("names the ship, where it is and the trade goods it pays out", () => {
    const view = describeUnitPickOption(scuttle, { id: "26|0" });
    expect(view?.title).toBe("Dreadnought II (damaged)");
    expect(view?.where).toBe("In the space of system 26");
    expect(view?.effect).toBe("Returned to your reinforcements; you gain 4 trade goods.");
    expect(describeUnitPickOption(scuttle, { id: "27|1" })?.effect).toContain("2 trade goods");
  });

  it("names the planet an infantry stands on and says it becomes a mech", () => {
    const view = describeUnitPickOption(refit, { id: "14|arinam|0" });
    expect(view?.where).toContain("system 14");
    expect(view?.effect).toBe("Replaced by a mech from your reinforcements.");
  });

  it("has nothing for the stop option or a decision without details", () => {
    expect(describeUnitPickOption(scuttle, { id: "stop" })).toBeNull();
    expect(describeUnitPickOption({ ...scuttle, details: undefined }, { id: "26|0" })).toBeNull();
  });

  it("shows the printed card", () => {
    const card = describeUnitPickCard(scuttle);
    expect(card?.name).toBe("Scuttle");
    expect(card?.text).not.toBe("");
    expect(describeUnitPickCard({ context: { subtype: "other" } as never })).toBeNull();
  });
});
