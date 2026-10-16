import { describe, expect, it } from "vitest";
import type { Choice, PurchaseHints, SessionUpdate } from "../wire";
import {
  countPurchase,
  payPurchase,
  purchasePlan,
  selectPurchase,
  setPurchaseGoods,
  togglePurchasePlanet,
} from "./purchase";

// The questions of Leadership as the engine asks them (the scenario "strategy_leadership").
const PURCHASE: PurchaseHints = {
  cost: 3,
  influence_available: 11,
  max: 3,
  planets: [
    { id: "jord", worth: 2 },
    { id: "qucenn", worth: 2 },
    { id: "rarron", worth: 3 },
  ],
  trade_good_worth: 1,
  trade_goods: 4,
  commodities: 0,
};
const pools = { tactic: 3, fleet: 3, strategic: 2 };
const gain = (left: number, purchase: PurchaseHints | null = PURCHASE): Choice => ({
  player: "a",
  prompt: "gain a command token into which pool",
  options: ["tactic_tokens", "fleet_tokens", "strategic_tokens"].map((id) => ({
    id,
    kind: "pool",
    label: id,
    payload: {},
  })),
  details: {
    kind: "command_tokens",
    mode: "gain",
    pools,
    ...(purchase ? { purchase } : {}),
    reinforcements: 7,
    tokens_to_place: left,
  },
});
const buy: Choice = {
  player: "a",
  prompt: "spend 3 influence for a command token",
  options: [
    { id: "no", kind: "strategy", label: "spend nothing further", payload: {} },
    { id: "yes", kind: "strategy", label: "spend 3 influence", payload: {} },
  ],
  details: { kind: "command_tokens", mode: "buy", pools, purchase: PURCHASE, tokens_to_place: 0 },
};
const update = {
  view: {
    board: {
      map_tiles: [
        { system_id: "01", planets: [{ id: "jord", label: "Jord" }] },
        {
          system_id: "29",
          planets: [
            { id: "qucenn", label: "Qucen'n" },
            { id: "rarron", label: "Rarron" },
          ],
        },
      ],
    },
  },
} as unknown as SessionUpdate;

describe("selectPurchase", () => {
  it("is null for a redistribution and for any other question", () => {
    const other: Choice = { ...buy, details: { ...buy.details, mode: "redistribute" } };
    expect(selectPurchase(update, other, null)).toBeNull();
    expect(selectPurchase(update, { ...buy, details: {} }, null)).toBeNull();
  });

  it("places the gained tokens in the pools, and nothing is bought at first", () => {
    const view = selectPurchase(update, gain(3), null)!;
    expect(view.blocks.map((block) => block.kind === "card" && block.title)).toEqual([
      "Buy command tokens",
      "Command pools",
    ]);
    expect(view.blocks[1]).toMatchObject({
      aside: "3 gained · 0 bought · 0 / 3 placed",
      bad: true,
    });
    expect(view.problem).toBe("Place 3 more tokens.");
    expect(view.task).toBeNull();
    // A decision of the pools: a phone has the screen on the map, with the payment.
    expect(view.onMap).toBe(true);
    expect(purchasePlan(gain(3), null)).toBeNull();
  });

  it("buys tokens, pays them on the map, and sends one plan", () => {
    const choice = gain(3);
    let draft = countPurchase(choice, null, "buy", 2)!;
    draft = countPurchase(choice, draft, "pools.t", 5)!;
    draft = countPurchase(choice, draft, "pools.f", 4)!;
    draft = countPurchase(choice, draft, "pools.s", 4)!;
    let view = selectPurchase(update, choice, draft)!;
    expect(view.blocks[1]).toMatchObject({
      aside: "3 gained · 2 bought · 5 / 5 placed",
      bad: false,
    });
    expect(view.problem).toBe("Pay 6 more influence.");
    expect(view.label).toBe("Buy 2 tokens");
    expect(view.task).toMatchObject({ kind: "pay", values: { jord: 2, qucenn: 2, rarron: 3 } });
    draft = togglePurchasePlanet(choice, draft, "rarron")!;
    draft = setPurchaseGoods(choice, draft, 3)!;
    view = selectPurchase(update, choice, draft)!;
    expect(view.problem).toBeNull();
    expect(view.payment).toMatchObject({ cost: 6, paid: 6, summary: "Rarron 3 + 3 trade goods" });
    expect(view.note).toBe("Tactic 3 → 5 · Fleet 3 → 4 · Strategy 2 → 4");
    expect(purchasePlan(choice, draft)).toEqual([
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "pool", pool: "fleet_tokens" },
      { kind: "purchase", buy: true },
      { kind: "pool", pool: "strategic_tokens" },
      { kind: "purchase", buy: true },
      { kind: "pool", pool: "strategic_tokens" },
      { kind: "purchase", buy: false },
      { kind: "pay", source: "exhaust|rarron" },
      { kind: "pay", source: "trade_good" },
      { kind: "pay", source: "trade_good" },
      { kind: "pay", source: "trade_good" },
    ]);
  });

  it("auto-pays with the least waste, and fewer bought tokens give the extra pools back", () => {
    let draft = countPurchase(buy, null, "buy", 1)!;
    draft = payPurchase(buy, draft, "auto")!;
    expect(draft).toMatchObject({ planets: ["rarron"], goods: 0 });
    draft = countPurchase(buy, draft, "pools.f", 4)!;
    expect(purchasePlan(buy, draft)).toEqual([
      { kind: "purchase", buy: true },
      { kind: "pool", pool: "fleet_tokens" },
      { kind: "purchase", buy: false },
      { kind: "pay", source: "exhaust|rarron" },
    ]);
    draft = countPurchase(buy, draft, "buy", 0)!;
    expect(draft).toMatchObject({ buy: 0, pools: null, planets: [], goods: 0 });
    expect(selectPurchase(update, buy, draft)!.label).toBe("Buy nothing");
    expect(purchasePlan(buy, draft)).toEqual([{ kind: "purchase", buy: false }]);
  });

  it("a gain with no purchase is the pools alone", () => {
    const choice = gain(1, null);
    const view = selectPurchase(update, choice, null)!;
    expect(view.blocks).toHaveLength(1);
    expect(view.onMap).toBe(true);
    const draft = countPurchase(choice, null, "pools.s", 3)!;
    expect(purchasePlan(choice, draft)).toEqual([{ kind: "pool", pool: "strategic_tokens" }]);
    expect(countPurchase(choice, null, "buy", 1)).toBeNull();
  });
});
