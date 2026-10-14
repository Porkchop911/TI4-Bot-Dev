import { describe, expect, it } from "vitest";
import { goodsNote, goodsOnCard, investmentsProgress, isStrategyCardGrid } from "./strategyGoods.ts";

const investments = {
  context: { subtype: "investments_pick_strategy_card" } as never,
  details: { step: 2, of: 5, distinct_owed: 2 },
};
const draft = { context: { subtype: "draft_strategy_card" } as never, details: undefined };
const table = { table: { strategy_card_goods: { pok1leadership: 2 } } } as never;

describe("strategy goods", () => {
  it("uses the card grid for the draft and for Manipulate Investments only", () => {
    expect(isStrategyCardGrid(draft)).toBe(true);
    expect(isStrategyCardGrid(investments)).toBe(true);
    expect(isStrategyCardGrid({ context: { subtype: "diplomacy_choose_system" } as never })).toBe(false);
  });

  it("reads the progress of the five placements", () => {
    expect(investmentsProgress(investments)).toEqual({ step: 2, of: 5, distinctOwed: 2 });
    expect(investmentsProgress(draft)).toBeNull();
    expect(investmentsProgress({ ...investments, details: {} })).toBeNull();
  });

  it("counts the trade goods on a card, zero when none lie there, unknown without a table", () => {
    expect(goodsOnCard(table, "pok1leadership")).toBe(2);
    expect(goodsOnCard(table, "pok2diplomacy")).toBe(0);
    expect(goodsOnCard(null, "pok1leadership")).toBeNull();
  });

  it("says what a placement or a pick does to the goods", () => {
    expect(goodsNote(investments, table, "pok1leadership")).toBe("2 trade goods on it now; placing one makes it 3.");
    expect(goodsNote(investments, table, "pok2diplomacy")).toContain("No trade goods on it yet");
    expect(goodsNote(draft, table, "pok1leadership")).toContain("taking this card collects them");
    expect(goodsNote(draft, null, "pok1leadership")).toBeNull();
  });
});
