import { describe, expect, it } from "vitest";
import { describeCommodityStanding } from "./commodityStanding.ts";

describe("describeCommodityStanding", () => {
  it("reads one seat's commodities, limit and trade goods", () => {
    expect(
      describeCommodityStanding({ details: { commodities: { have: 1, max: 4, trade_goods: 3 } } }),
    ).toEqual({ have: 1, max: 4, tradeGoods: 3 });
  });

  it("is null without figures, and for the Trade primary's per-seat map", () => {
    expect(describeCommodityStanding({})).toBeNull();
    expect(
      describeCommodityStanding({ details: { commodities: { hacan: { have: 1, max: 6 } } } }),
    ).toBeNull();
  });

  it("leaves trade goods out when the engine sent none", () => {
    expect(describeCommodityStanding({ details: { commodities: { have: 0, max: 2 } } })).toEqual({
      have: 0,
      max: 2,
      tradeGoods: null,
    });
  });
});
