import { describe, expect, it } from "vitest";
import { isStrategyCardGrid } from "./strategyGoods.ts";
import { describeStrategyCardPick, strategyCardHolder } from "./strategyCardPick.ts";

const overrule = {
  context: { subtype: "overrule_pick_strategy card" } as never,
  details: { holders: { te4construction: "player_ab", pok6warfare: null } },
};

describe("strategy card picks of action cards", () => {
  it("shows Public Disgrace, Overrule and Strategize as the strategy-card grid", () => {
    expect(isStrategyCardGrid(overrule)).toBe(true);
    expect(isStrategyCardGrid({ context: { subtype: "public_disgrace_choose_card" } as never })).toBe(true);
    expect(isStrategyCardGrid({ context: { subtype: "strategize3_pick_strategy card" } as never })).toBe(true);
    expect(isStrategyCardGrid({ context: { subtype: "spy_pick_player" } as never })).toBe(false);
  });

  it("names the card, the half it performs and, for Disgrace, who took which card", () => {
    expect(describeStrategyCardPick(overrule)).toMatchObject({ kind: "overrule", half: "primary" });
    expect(describeStrategyCardPick({ context: { subtype: "strategize2_pick_strategy card" } as never })).toMatchObject({
      kind: "strategize",
      half: "secondary",
    });
    const disgrace = describeStrategyCardPick({
      context: { subtype: "public_disgrace_choose_card" } as never,
      details: { first: "pok5trade", played_by: "player_cd" },
    });
    expect(disgrace).toMatchObject({ kind: "disgrace", half: null, playedBy: "player_cd", takenCard: "Trade" });
    expect(describeStrategyCardPick({ context: { subtype: "draft_strategy_card" } as never })).toBeNull();
  });

  it("tells who holds each card, or that it is unchosen", () => {
    expect(strategyCardHolder(overrule, { id: "te4construction" })).toBe("player_ab");
    expect(strategyCardHolder(overrule, { id: "pok6warfare" })).toBeNull();
    expect(strategyCardHolder(overrule, { id: "pok1leadership" })).toBeUndefined();
    expect(strategyCardHolder({ context: overrule.context }, { id: "x" })).toBeUndefined();
  });
});
