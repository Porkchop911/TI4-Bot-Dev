import { describe, expect, it } from "vitest";
import { describeNumberStepper } from "./numberStepper.ts";

const counts = (n: number) =>
  Array.from({ length: n + 1 }, (_, i) => ({ id: String(i), kind: "count", label: `spend ${i} trade goods`, payload: {} }));
const bribery = (n: number) => ({
  context: { subtype: "bribery_pick_count", source: { ActionCard: "bribery" } } as never,
  options: counts(n),
});

describe("Bribery's number stepper", () => {
  it("covers 0 to the trade goods you hold", () => {
    const view = describeNumberStepper(bribery(4))!;
    expect([view.min, view.max, view.cardName]).toEqual([0, 4, "Bribery"]);
    expect(view.optionFor(3)?.id).toBe("3");
    expect(view.optionFor(9)).toBeUndefined();
  });

  it("says what each count buys", () => {
    const view = describeNumberStepper(bribery(4))!;
    expect(view.describe(0)).toMatch(/Spend nothing/);
    expect(view.describe(1)).toBe("Spend 1 trade good: cast 1 additional vote for the outcome you voted on.");
    expect(view.describe(3)).toContain("3 trade goods");
  });

  it("is nothing for other decisions or a list with gaps", () => {
    expect(describeNumberStepper({ context: { subtype: "other" } as never, options: counts(2) })).toBeNull();
    expect(describeNumberStepper({ ...bribery(3), options: [counts(3)[0], counts(3)[3]] })).toBeNull();
  });
});
