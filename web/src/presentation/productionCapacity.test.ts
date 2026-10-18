import { describe, expect, it } from "vitest";
import type { ChoiceOption } from "../protocol/types.ts";
import { capacityMeter, capacityMeterText, readCapacityFacts, readCapacityNow } from "./productionCapacity.ts";

const opt = (
  unit: string,
  role: string,
  provided: number,
  needed: number,
  placement: string,
): ChoiceOption => ({
  id: `build|${unit}|1`,
  kind: "produce",
  label: unit,
  payload: { unit, capacity_provided: provided, capacity_needed: needed, capacity_role: role, space_placement: placement },
});
const carrier = opt("carrier", "none", 4, 0, "always");
const fighter = opt("fighter", "fighter", 0, 2, "always");
const infantry = opt("infantry", "ground", 0, 2, "never");
const options = [carrier, fighter, infantry];
const now = (over: object = {}) => readCapacityNow({ transport: 0, consumed: 0, fighters: 0, unused_support: 0, ...over })!;

describe("capacityMeter", () => {
  it("is empty with nothing there and nothing staged", () => {
    const m = capacityMeter(now(), options, {});
    expect([m.transport, m.consumed, m.over]).toEqual([0, 0, 0]);
    expect(capacityMeterText(m)).toBe("Capacity 0 / 0");
  });

  it("adds the capacity of a staged carrier", () => {
    const m = capacityMeter(now({ transport: 2, consumed: 1, fighters: 1 }), options, { [carrier.id]: 1 });
    expect([m.existing, m.fromQueued, m.transport, m.over]).toEqual([2, 4, 6, 0]);
  });

  it("flags fighters beyond capacity and says they would be removed", () => {
    const m = capacityMeter(now({ transport: 2, consumed: 2, fighters: 2 }), options, { [fighter.id]: 1 });
    expect([m.consumed, m.transport, m.over, m.overKind]).toEqual([4, 2, 2, "fighters"]);
    expect(capacityMeterText(m)).toBe("Capacity 4 / 2 (2 fighters over: they would be removed)");
  });

  it("a carrier staged beside the fighters cancels the excess", () => {
    const m = capacityMeter(now(), options, { [fighter.id]: 2, [carrier.id]: 1 });
    expect([m.consumed, m.transport, m.over]).toEqual([4, 4, 0]);
  });

  it("uses dock fighter support before capacity, for fighters only", () => {
    const m = capacityMeter(now({ unused_support: 3 }), options, { [fighter.id]: 1 });
    expect([m.consumed, m.over]).toEqual([0, 0]);
    const g = capacityMeter(now({ unused_support: 3 }), [fighter, { ...infantry, payload: { ...infantry.payload, space_placement: "always" } }], {
      [infantry.id]: 1,
    });
    expect([g.consumed, g.over, g.overKind]).toEqual([2, 2, "units"]);
  });

  it("ground forces landing on a planet take no capacity", () => {
    const m = capacityMeter(now(), options, { [infantry.id]: 2 });
    expect([m.consumed, m.maybeGround]).toEqual([0, 0]);
  });

  it("counts ground forces in a planetless system (space is the only destination)", () => {
    const spaceInfantry = opt("infantry", "ground", 0, 2, "always");
    const m = capacityMeter(now(), [spaceInfantry], { [spaceInfantry.id]: 1 });
    expect([m.consumed, m.over, m.queuedGround]).toEqual([2, 2, 2]);
  });

  it("keeps a ground force with a choice of destination out of the total and reports it apart", () => {
    const either = opt("infantry", "ground", 0, 2, "choice");
    const m = capacityMeter(now(), [either], { [either.id]: 1 });
    expect([m.consumed, m.maybeGround]).toEqual([0, 2]);
  });
});

describe("readers", () => {
  it("refuses malformed data", () => {
    expect(readCapacityNow(null)).toBeNull();
    expect(readCapacityNow({ transport: "x" })).toBeNull();
    expect(readCapacityFacts({ id: "a", kind: "produce", label: "a", payload: {} })).toBeNull();
  });
});
