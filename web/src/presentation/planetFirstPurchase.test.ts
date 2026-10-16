import { describe, expect, it } from "vitest";
import {
  NO_PURCHASE,
  chooseTokenCount,
  describeCommandTokens,
  initialStaging,
  paymentInfluence,
  planPayment,
  purchaseFromPayment,
  purchaseSummary,
  purchasesFor,
  stagedPayment,
  stepPaymentGoods,
  suggestedPurchase,
  togglePaymentPlanet,
  tokenOutcome,
  addToken,
  type CommandTokenView,
  type PurchaseState,
} from "./commandTokens.ts";

const pools = ["tactic_tokens", "fleet_tokens", "strategic_tokens"].map((id) => ({ id, kind: "pool", label: id }));

const view = (over: { reinforcements?: number; free?: number; tradeGoods?: number } = {}): CommandTokenView => {
  const resources = new Map([
    ["jord", 2],
    ["arcturus", 0],
    ["lodor", 3],
    ["pure", 0],
  ]);
  const planets = [
    { id: "jord", worth: 2 },
    { id: "arcturus", worth: 4 },
    { id: "lodor", worth: 3 },
    { id: "pure", worth: 2 },
  ];
  const goods = over.tradeGoods ?? 2;
  const free = over.free ?? 1;
  const built = describeCommandTokens(
    {
      options: free === 0 ? [{ id: "yes", kind: "yes", label: "yes" }, { id: "no", kind: "no", label: "no" }] : pools,
      details: {
        kind: free === 0 ? "strategy_secondary" : "command_tokens",
        mode: free === 0 ? "buy" : "gain",
        pools: { tactic: 3, fleet: 4, strategic: 2 },
        reinforcements: over.reinforcements ?? 20,
        tokens_to_place: over.free ?? 1,
        purchase: {
          cost: 3,
          influence_available: 11 + goods,
          max: Math.floor((11 + goods) / 3),
          trade_goods: goods,
          trade_good_worth: 1,
          planets,
        },
      },
    },
    true,
    resources,
  );
  if (!built) throw new Error("no view");
  return built;
};

const click = (v: CommandTokenView, s: PurchaseState, ...ids: string[]) =>
  ids.reduce((state, id) => togglePaymentPlanet(v, state, id), s);

describe("planet-first purchase state machine", () => {
  it("starts with nothing staged and no tokens bought", () => {
    expect(NO_PURCHASE).toEqual({ bought: 0, override: null });
    expect(stagedPayment(view(), NO_PURCHASE)).toEqual({ planetIds: [], tradeGoods: 0 });
  });

  it("carries each planet's resource value from the board", () => {
    const v = view();
    expect(v.purchase!.planets.map((p) => [p.id, p.resources])).toEqual([
      ["jord", 2],
      ["arcturus", 0],
      ["lodor", 3],
      ["pure", 0],
    ]);
  });

  it("derives the tokens from the selection: 3 -> 1, 5 -> 1, 6 -> 2, 7 -> 2, 4 -> 1", () => {
    const v = view();
    const bought = (...ids: string[]) => click(v, NO_PURCHASE, ...ids).bought;
    expect(bought("lodor")).toBe(1); // 3
    expect(bought("lodor", "jord")).toBe(1); // 5
    expect(bought("arcturus", "jord")).toBe(2); // 6
    expect(bought("arcturus", "lodor")).toBe(2); // 7
    expect(bought("arcturus")).toBe(1); // 4
    expect(bought("jord")).toBe(0); // 2
  });

  it("wasted influence is the selection beyond what the tokens cost", () => {
    const v = view();
    const wasted = (...ids: string[]) => purchaseSummary(v, click(v, NO_PURCHASE, ...ids)).wasted;
    expect(wasted("lodor")).toBe(0);
    expect(wasted("lodor", "jord")).toBe(2);
    expect(wasted("arcturus", "lodor")).toBe(1);
  });

  it("deselecting the last planet is no purchase again", () => {
    const v = view();
    expect(click(v, NO_PURCHASE, "lodor", "lodor")).toEqual(NO_PURCHASE);
  });

  it("trade goods count toward the influence and are limited to what is held", () => {
    const v = view({ tradeGoods: 2 });
    const withGoods = stepPaymentGoods(v, click(v, NO_PURCHASE, "jord"), 1);
    expect(paymentInfluence(v, stagedPayment(v, withGoods))).toBe(3);
    expect(withGoods.bought).toBe(1);
    const capped = stepPaymentGoods(v, stepPaymentGoods(v, withGoods, 1), 5);
    expect(stagedPayment(v, capped).tradeGoods).toBe(2);
    expect(stepPaymentGoods(v, NO_PURCHASE, -1)).toEqual(NO_PURCHASE);
  });

  it("the token count is capped by the reinforcements after the free tokens", () => {
    const v = view({ reinforcements: 4, free: 1 });
    expect(purchasesFor(v, { planetIds: ["arcturus", "lodor", "jord", "pure"], tradeGoods: 0 })).toBe(3);
    const tight = view({ reinforcements: 3, free: 1 });
    expect(purchasesFor(tight, { planetIds: ["arcturus", "lodor", "jord", "pure"], tradeGoods: 0 })).toBe(2);
    expect(purchaseSummary(tight, purchaseFromPayment(tight, { planetIds: ["arcturus", "lodor", "jord", "pure"], tradeGoods: 0 })).atLimit).toBe(true);
  });

  it("'Buy N tokens' pre-stages the suggested payment and clamps to what is affordable", () => {
    const v = view();
    const two = chooseTokenCount(v, 2);
    expect(two).toEqual({ bought: 2, override: null });
    const plan = planPayment(v, 2)!;
    expect(stagedPayment(v, two).planetIds).toEqual(plan.planets.map((p) => p.id));
    expect(chooseTokenCount(v, 99).bought).toBe(4);
    expect(chooseTokenCount(v, -3).bought).toBe(0);
  });

  it("the one-tap suggestion is for the most that can be bought, or for the tokens already bought", () => {
    const v = view();
    const first = suggestedPurchase(v, NO_PURCHASE);
    expect(first.bought).toBeGreaterThan(0);
    expect(suggestedPurchase(v, { bought: 1, override: { planetIds: ["lodor"], tradeGoods: 0 } })).toEqual({
      bought: 1,
      override: null,
    });
  });

  it("suggested planets for one token are the zero-resource ones, not the exact resource planet", () => {
    const v = view();
    const one = chooseTokenCount(v, 1);
    // lodor pays exactly 3 but has 3 resources; arcturus (4, 0 resources) wastes 1 and exhausts none.
    expect(stagedPayment(v, one).planetIds).toEqual(["arcturus"]);
  });

  it("selecting planets, assigning pools and confirming sends the same tokens batch as the old flow", () => {
    const v = view({ free: 1 });
    const state = click(v, NO_PURCHASE, "arcturus", "jord"); // 6 influence -> 2 tokens
    expect(state.bought).toBe(2);
    let staging = initialStaging(v);
    for (const pool of ["tactic", "tactic", "fleet"] as const) staging = addToken(v, staging, pool, state.bought);
    const outcome = tokenOutcome(v, staging, state.bought, state.override);
    expect(outcome).toEqual({
      kind: "plan",
      steps: [
        { kind: "pool", pool: "tactic_tokens" },
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "arcturus" },
        { kind: "pool", pool: "tactic_tokens" },
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "jord" },
        { kind: "pool", pool: "fleet_tokens" },
        { kind: "purchase", buy: false },
      ],
    });
  });

  it("a selection that buys no token cannot be confirmed", () => {
    const v = view({ free: 0 });
    const state = click(v, NO_PURCHASE, "jord");
    expect(state.bought).toBe(0);
    expect(tokenOutcome(v, initialStaging(v), state.bought, state.override)).toBeNull();
    expect(tokenOutcome(v, initialStaging(v), 0, null)).not.toBeNull();
  });

  it("the old 'Buy N' and the new planet-first routes meet: the same selection gives the same wire steps", () => {
    const v = view({ free: 0 });
    const viaCount = chooseTokenCount(v, 1);
    const viaPlanets = click(v, NO_PURCHASE, ...stagedPayment(v, viaCount).planetIds);
    let staging = initialStaging(v);
    staging = addToken(v, staging, "strategic", 1);
    expect(tokenOutcome(v, staging, viaCount.bought, viaCount.override)).toEqual(
      tokenOutcome(v, staging, viaPlanets.bought, viaPlanets.override),
    );
  });
});
