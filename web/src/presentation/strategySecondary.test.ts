import { describe, expect, it } from "vitest";
import { describeStrategySecondary, secondaryButtonLabel } from "./strategySecondary.ts";

const options = [
  { id: "no", label: "decline", kind: "strategy", payload: {} },
  { id: "yes", label: "draw", kind: "strategy", payload: {} },
];

describe("secondaryButtonLabel", () => {
  it("names the cost and the effect", () => {
    expect(secondaryButtonLabel("spend a strategy token to draw two action cards", "draw")).toBe(
      "Spend 1 strategy token to draw two action cards",
    );
    expect(
      secondaryButtonLabel("spend a strategy token and 4 resources to research", "spend"),
    ).toBe("Spend 1 strategy token + 4 resources to research");
    expect(secondaryButtonLabel("spend 3 influence for a command token", "yes")).toBe(
      "Spend 3 influence for a command token",
    );
    expect(secondaryButtonLabel("something else", "go")).toBe("go");
  });
});

describe("describeStrategySecondary", () => {
  const choice = {
    prompt: "spend a strategy token to draw two action cards",
    options,
    details: {
      kind: "strategy_secondary",
      card: "pok3politics",
      played_by: "seat_b",
      tokens_left: 3,
      costs_token: true,
    },
  };

  it("shows the card, who played it, its secondary text and the tokens left", () => {
    const view = describeStrategySecondary(choice);
    expect(view?.cardName).toBe("Politics");
    expect(view?.playedBy).toBe("seat_b");
    expect(view?.secondaryText).toBeTruthy();
    expect(view?.tokensLeft).toBe(3);
    expect(view?.yes.id).toBe("yes");
    expect(view?.no.id).toBe("no");
    expect(view?.yesLabel).toBe("Spend 1 strategy token to draw two action cards");
  });

  it("is not a secondary without the server's details or without a yes/no pair", () => {
    expect(describeStrategySecondary({ ...choice, details: undefined })).toBeNull();
    expect(
      describeStrategySecondary({ ...choice, options: [...options, { id: "x", label: "x", payload: {}, kind: "" }] }),
    ).toBeNull();
  });

  it("carries the Trade follower's commodities when the engine sent them", () => {
    const trade = describeStrategySecondary({
      prompt: "spend a strategy token to replenish commodities",
      options: [
        { id: "no", label: "decline", kind: "strategy", payload: {} },
        { id: "yes", label: "replenish", kind: "strategy", payload: {} },
      ],
      details: {
        kind: "strategy_secondary",
        card: "pok5trade",
        played_by: "seat_b",
        tokens_left: 2,
        costs_token: true,
        commodities: { have: 1, max: 4, trade_goods: 0 },
      },
    });
    expect(trade?.commodities).toEqual({ have: 1, max: 4, tradeGoods: 0 });
    expect(describeStrategySecondary(choice)?.commodities).toBeNull();
  });
});
