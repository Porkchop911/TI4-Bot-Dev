import { describe, expect, it } from "vitest";
import {
  boughtObjectiveUnit,
  boughtProgressText,
  isBoughtObjective,
} from "./boughtObjective.ts";

describe("boughtObjectiveUnit", () => {
  it("reads the unit from the printed cost", () => {
    expect(boughtObjectiveUnit("Spend 8 resources.")).toBe("resources");
    expect(boughtObjectiveUnit("Spend 16 influence.")).toBe("influence");
    expect(boughtObjectiveUnit("Spend 10 trade goods.")).toBe("trade goods");
  });

  it("is null for other objectives", () => {
    expect(boughtObjectiveUnit("Have 7 or more structures.")).toBeNull();
    expect(boughtObjectiveUnit("Spend a total of 6 tokens from your tactic and/or strategy pools.")).toBeNull();
    expect(boughtObjectiveUnit(undefined)).toBeNull();
  });
});

describe("isBoughtObjective", () => {
  it("includes the mixed spend but not token spends", () => {
    expect(isBoughtObjective("Spend 3 influence, 3 resources, and 3 trade goods.")).toBe(true);
    expect(isBoughtObjective("Spend 8 influence.")).toBe(true);
    expect(isBoughtObjective("Spend a total of 3 tokens from your tactic and/or strategy pools.")).toBe(false);
  });
});

describe("boughtProgressText", () => {
  it("marks capacity outside a scoring window", () => {
    expect(boughtProgressText(5, 8, "resources", false)).toBe("can pay 5 / 8 resources now · pay at scoring");
  });

  it("drops the suffix inside a scoring window", () => {
    expect(boughtProgressText(3, 8, "influence", true)).toBe("can pay 3 / 8 influence now");
  });

  it("omits the unit when there is none", () => {
    expect(boughtProgressText(2, 3, null, true)).toBe("can pay 2 / 3 now");
  });
});
