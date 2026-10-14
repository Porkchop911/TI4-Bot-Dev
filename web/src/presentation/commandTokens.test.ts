import { describe, expect, it } from "vitest";
import {
  addToken,
  arrangementId,
  canConfirmTokens,
  confirmBlocker,
  describeCommandTokens,
  gainTokensPrompt,
  type CommandTokenView,
  fitStaging,
  initialStaging,
  maxPurchases,
  overrideFromPlan,
  overrideIsValid,
  paymentCheck,
  planPayment,
  poolPips,
  removeToken,
  restackMoves,
  resultingCount,
  tokenOutcome,
  tokenPlan,
  tokenPlanWithPurchase,
  tokensRemaining,
} from "./commandTokens.ts";

const poolOptions = ["tactic_tokens", "fleet_tokens", "strategic_tokens"].map((id) => ({
  id,
  kind: "pool",
  label: id,
}));
const gain = (toPlace = 2) => ({
  options: poolOptions,
  details: {
    kind: "command_tokens",
    mode: "gain",
    pools: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 7,
    tokens_to_place: toPlace,
  },
});
const redistribute = (arrangements = ["3|4|2", "2|5|2", "4|4|1", "9|0|0"]) => ({
  options: arrangements.map((id) => ({ id, kind: "redistribute", label: id })),
  details: {
    kind: "command_tokens",
    mode: "redistribute",
    pools: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 7,
    total: 9,
  },
});

describe("describeCommandTokens", () => {
  it("reads a gain: pools, reinforcements and how many to place", () => {
    const view = describeCommandTokens(gain(), true)!;
    expect(view.mode).toBe("gain");
    expect(view.current).toEqual({ tactic: 3, fleet: 4, strategic: 2 });
    expect(view.reinforcements).toBe(7);
    expect(view.total).toBe(2);
  });

  it("offers a gain only with a batch submitter, so a plain list remains the fallback", () => {
    expect(describeCommandTokens(gain(), false)).toBeNull();
  });

  it("offers a redistribution without a batch submitter", () => {
    const view = describeCommandTokens(redistribute(), false)!;
    expect(view.mode).toBe("redistribute");
    expect(view.total).toBe(9);
    expect(view.arrangements.has("2|5|2")).toBe(true);
  });

  it("ignores other decisions and incomplete details", () => {
    expect(describeCommandTokens({ options: poolOptions }, true)).toBeNull();
    expect(
      describeCommandTokens({ options: poolOptions, details: { kind: "strategy_secondary" } }, true),
    ).toBeNull();
    const noPools = { ...gain(), details: { kind: "command_tokens", mode: "gain" } };
    expect(describeCommandTokens(noPools, true)).toBeNull();
    expect(describeCommandTokens({ ...gain(), options: poolOptions.slice(0, 2) }, true)).toBeNull();
  });
});

describe("gain staging", () => {
  const view = describeCommandTokens(gain(2), true)!;

  it("counts down the tokens left and stops adding at zero", () => {
    let staging = initialStaging(view);
    expect(tokensRemaining(view, staging)).toBe(2);
    staging = addToken(view, staging, "fleet");
    staging = addToken(view, staging, "fleet");
    expect(tokensRemaining(view, staging)).toBe(0);
    expect(addToken(view, staging, "tactic")).toEqual(staging);
    expect(resultingCount(view, staging, "fleet")).toBe(6);
  });

  it("confirms only when every token is assigned", () => {
    let staging = addToken(view, initialStaging(view), "tactic");
    expect(canConfirmTokens(view, staging)).toBe(false);
    expect(confirmBlocker(view, staging)).toContain("1 more token");
    expect(tokenOutcome(view, staging)).toBeNull();
    staging = addToken(view, staging, "strategic");
    expect(canConfirmTokens(view, staging)).toBe(true);
  });

  it("removes only what was staged, never below zero", () => {
    const staging = addToken(view, initialStaging(view), "tactic");
    expect(removeToken(staging, "fleet")).toEqual(staging);
    expect(removeToken(staging, "tactic")).toEqual(initialStaging(view));
  });

  it("plans one pool step per token in pool order", () => {
    let staging = initialStaging(view);
    staging = addToken(view, staging, "strategic");
    staging = addToken(view, staging, "tactic");
    expect(tokenPlan(staging)).toEqual([
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "pool", pool: "strategic_tokens" },
    ]);
    expect(tokenOutcome(view, staging)).toEqual({ kind: "plan", steps: tokenPlan(staging) });
  });

  it("shows existing pips apart from new ones", () => {
    const staging = addToken(view, initialStaging(view), "tactic");
    expect(poolPips(view, staging, "tactic")).toEqual({ kept: 3, added: 1, removed: 0 });
    expect(poolPips(view, staging, "fleet")).toEqual({ kept: 4, added: 0, removed: 0 });
  });
});

describe("redistribution staging", () => {
  const view = describeCommandTokens(redistribute(), false)!;

  it("starts as the current arrangement with nothing left to assign", () => {
    const staging = initialStaging(view);
    expect(staging).toEqual({ tactic: 3, fleet: 4, strategic: 2 });
    expect(tokensRemaining(view, staging)).toBe(0);
    expect(canConfirmTokens(view, staging)).toBe(true);
  });

  it("frees a token by removing it and must reassign it before confirming", () => {
    let staging = removeToken(initialStaging(view), "strategic");
    expect(tokensRemaining(view, staging)).toBe(1);
    expect(canConfirmTokens(view, staging)).toBe(false);
    staging = addToken(view, staging, "fleet");
    // 3|5|1 is not among the offered arrangements.
    expect(arrangementId(view, staging)).toBeNull();
    expect(canConfirmTokens(view, staging)).toBe(false);
    expect(tokenOutcome(view, { tactic: 2, fleet: 5, strategic: 2 })).toEqual({
      kind: "option",
      optionId: "2|5|2",
    });
  });

  it("refuses an arrangement the engine does not offer, with a reason", () => {
    const bad = { tactic: 8, fleet: 1, strategic: 0 };
    expect(arrangementId(view, bad)).toBeNull();
    expect(confirmBlocker(view, bad)).toContain("fleet pool");
    expect(tokenOutcome(view, bad)).toBeNull();
  });

  it("shows tokens moved out as removed pips and tokens moved in as new ones", () => {
    const staging = { tactic: 2, fleet: 5, strategic: 2 };
    expect(poolPips(view, staging, "tactic")).toEqual({ kept: 2, added: 0, removed: 1 });
    expect(poolPips(view, staging, "fleet")).toEqual({ kept: 4, added: 1, removed: 0 });
    expect(resultingCount(view, staging, "fleet")).toBe(5);
  });
});

const purchaseDetails = (over: Record<string, unknown> = {}) => ({
  cost: 3,
  influence_available: 9,
  max: 3,
  trade_goods: 7,
  trade_good_worth: 1,
  planets: [{ id: "jord", worth: 2, resources: null }],
  ...over,
});
const gainBuy = (toPlace = 3, purchase = purchaseDetails()) => ({
  options: poolOptions,
  details: { ...gain(toPlace).details, purchase },
});
const buyQuestion = (purchase = purchaseDetails()) => ({
  options: [
    { id: "no", kind: "strategy", label: "spend nothing further" },
    { id: "yes", kind: "strategy", label: "spend 3 influence" },
  ],
  details: {
    kind: "command_tokens",
    mode: "buy",
    pools: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 7,
    tokens_to_place: 0,
    purchase,
  },
});
const secondaryWindow = () => ({
  ...buyQuestion(),
  details: { ...buyQuestion().details, kind: "strategy_secondary", card: "pok1leadership" },
});
const staged = (tactic: number, fleet: number, strategic: number) => ({ tactic, fleet, strategic });

describe("gain + buy model", () => {
  it("reads the purchase from the free gain, the buy question and the secondary window", () => {
    for (const choice of [gainBuy(), buyQuestion(), secondaryWindow()]) {
      const view = describeCommandTokens(choice, true)!;
      expect(view.purchase).toMatchObject({ cost: 3, influence: 9, max: 3, tradeGoods: 7 });
    }
    expect(describeCommandTokens(gainBuy(), true)!.total).toBe(3);
    expect(describeCommandTokens(buyQuestion(), true)!.total).toBe(0);
    expect(describeCommandTokens(gain(), true)!.purchase).toBeNull();
  });

  it("falls back to the per-decision flow without a batch submitter or a usable purchase", () => {
    expect(describeCommandTokens(gainBuy(), false)).toBeNull();
    expect(describeCommandTokens(buyQuestion(), false)).toBeNull();
    const broken = { ...gainBuy(), details: { ...gainBuy().details, purchase: { cost: 3 } } };
    expect(describeCommandTokens(broken, true)!.purchase).toBeNull();
    expect(describeCommandTokens({ ...secondaryWindow(), details: { kind: "strategy_secondary" } }, true)).toBeNull();
  });

  it("bounds the purchases by influence, spendable sources and the reinforcements", () => {
    expect(maxPurchases(describeCommandTokens(gainBuy(), true)!)).toBe(3);
    const poor = describeCommandTokens(gainBuy(3, purchaseDetails({ trade_goods: 1, max: 3 })), true)!;
    expect(maxPurchases(poor)).toBe(1); // jord 2 + one good
    const crowded = describeCommandTokens(
      { ...gainBuy(), details: { ...gainBuy().details, reinforcements: 4 } },
      true,
    )!;
    expect(maxPurchases(crowded)).toBe(1); // 3 free + 1 bought fill the reinforcements
  });

  it("pays like Auto-pay: planets with the least waste, goods only when they fall short", () => {
    const view = describeCommandTokens(gainBuy(), true)!;
    expect(planPayment(view, 0)).toMatchObject({ spent: 0, extra: 0 });
    expect(planPayment(view, 1)).toMatchObject({ planets: [{ id: "jord", worth: 2, resources: null }], tradeGoods: 1, spent: 3, extra: 0 });
    expect(planPayment(view, 3)).toMatchObject({ tradeGoods: 7, spent: 9 });
    expect(planPayment(view, 4)).toBeNull();
    const big = describeCommandTokens(
      gainBuy(3, purchaseDetails({ planets: [{ id: "a", worth: 4, resources: null }, { id: "b", worth: 1 }], influence_available: 12, max: 4 })),
      true,
    )!;
    // 3 influence: planets a (4) over-pays by one; a+b is worse; so a alone, and one is carried.
    expect(planPayment(big, 1)).toMatchObject({ planets: [{ id: "a", worth: 4, resources: null }], tradeGoods: 0, extra: 1 });
  });

  it("totals free plus bought tokens and gates Confirm on assigning all of them", () => {
    const view = describeCommandTokens(gainBuy(), true)!;
    expect(tokensRemaining(view, staged(0, 0, 0), 2)).toBe(5);
    expect(canConfirmTokens(view, staged(1, 1, 1), 0)).toBe(true);
    expect(canConfirmTokens(view, staged(1, 1, 1), 1)).toBe(false);
    expect(confirmBlocker(view, staged(1, 1, 1), 1)).toBe("Assign 1 more token to confirm.");
    expect(canConfirmTokens(view, staged(2, 2, 1), 2)).toBe(true);
    expect(canConfirmTokens(view, staged(0, 0, 0), 4)).toBe(false);
  });

  it("drops staged tokens when fewer are bought", () => {
    const view = describeCommandTokens(gainBuy(), true)!;
    expect(fitStaging(view, staged(2, 2, 1), 1)).toEqual(staged(2, 2, 0));
    expect(fitStaging(view, staged(2, 2, 1), 0)).toEqual(staged(2, 1, 0));
  });

  it("plans free pools, then per purchase: yes, payment, pool, and the closing no", () => {
    const view = describeCommandTokens(gainBuy(), true)!;
    const steps = tokenPlanWithPurchase(view, staged(1, 2, 2), 2)!;
    expect(steps).toEqual([
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "pool", pool: "fleet_tokens" },
      { kind: "pool", pool: "fleet_tokens" },
      { kind: "purchase", buy: true },
      { kind: "exhaust", planet: "jord" },
      { kind: "trade_good" },
      { kind: "pool", pool: "strategic_tokens" },
      { kind: "purchase", buy: true },
      { kind: "trade_good" },
      { kind: "trade_good" },
      { kind: "trade_good" },
      { kind: "pool", pool: "strategic_tokens" },
      { kind: "purchase", buy: false },
    ]);
  });

  it("asks no closing no when the purchases use up everything affordable", () => {
    const view = describeCommandTokens(gainBuy(), true)!;
    const steps = tokenPlanWithPurchase(view, staged(3, 3, 0), 3)!;
    expect(steps.filter((step) => step.kind === "purchase")).toHaveLength(3);
    expect(steps.at(-1)).toEqual({ kind: "pool", pool: "fleet_tokens" });
  });

  it("carries an overpayment into the next token instead of paying twice", () => {
    const view = describeCommandTokens(
      gainBuy(3, purchaseDetails({ planets: [{ id: "big", worth: 6, resources: null }], influence_available: 6, max: 2, trade_goods: 0 })),
      true,
    )!;
    const steps = tokenPlanWithPurchase(view, staged(3, 2, 0), 2)!;
    expect(steps.slice(3)).toEqual([
      { kind: "purchase", buy: true },
      { kind: "exhaust", planet: "big" },
      { kind: "pool", pool: "fleet_tokens" },
      { kind: "purchase", buy: true },
      { kind: "pool", pool: "fleet_tokens" },
    ]);
  });

  it("answers a bare purchase question: nothing bought is one no, a buy starts with yes", () => {
    const view = describeCommandTokens(buyQuestion(), true)!;
    expect(canConfirmTokens(view, staged(0, 0, 0), 0)).toBe(true);
    expect(tokenOutcome(view, staged(0, 0, 0), 0)).toEqual({
      kind: "plan",
      steps: [{ kind: "purchase", buy: false }],
    });
    const bought = tokenOutcome(view, staged(0, 1, 0), 1);
    expect(bought).toEqual({
      kind: "plan",
      steps: [
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "jord" },
        { kind: "trade_good" },
        { kind: "pool", pool: "fleet_tokens" },
        { kind: "purchase", buy: false },
      ],
    });
  });
});

describe("payment override", () => {
  const view = (): CommandTokenView => ({
    mode: "gain",
    current: { tactic: 3, fleet: 4, strategic: 2 },
    reinforcements: 20,
    total: 0,
    arrangements: new Set(),
    purchase: {
      cost: 3,
      influence: 14,
      max: 4,
      planets: [
        { id: "jord", worth: 2, resources: null },
        { id: "arcturus", worth: 4, resources: null },
        { id: "lodor", worth: 3, resources: null },
      ],
      tradeGoods: 2,
      tradeGoodWorth: 1,
    },
  });
  const staged = { tactic: 1, fleet: 0, strategic: 0 };

  it("Auto-pay stays the default and its plan is unchanged without an override", () => {
    const v = view();
    expect(paymentCheck(v, 1, null).plan).toEqual(planPayment(v, 1));
    expect(paymentCheck(v, 1, null).problem).toBeNull();
    expect(tokenPlanWithPurchase(v, staged, 1)).toEqual(tokenPlanWithPurchase(v, staged, 1, null));
  });

  it("accepts any affordable combination that the bill needs, with the account", () => {
    const v = view();
    const exact = paymentCheck(v, 1, { planetIds: ["lodor"], tradeGoods: 0 });
    expect([exact.paid, exact.bill, exact.remainder, exact.waste, exact.problem]).toEqual([3, 3, 0, 0, null]);
    // The last item may overshoot: the engine stops asking once the bill is covered.
    const waste = paymentCheck(v, 1, { planetIds: ["arcturus"], tradeGoods: 0 });
    expect([waste.paid, waste.waste, waste.problem]).toEqual([4, 1, null]);
    const mixed = paymentCheck(v, 1, { planetIds: ["jord"], tradeGoods: 1 });
    expect([mixed.paid, mixed.problem]).toEqual([3, null]);
  });

  it("rejects an insufficient payment and one with an item the bill did not need", () => {
    const v = view();
    const short = paymentCheck(v, 1, { planetIds: ["jord"], tradeGoods: 0 });
    expect([short.remainder, short.problem]).toEqual([1, expect.stringContaining("Short by 1")]);
    const extra = paymentCheck(v, 1, { planetIds: ["lodor", "jord"], tradeGoods: 0 });
    expect(extra.problem).toContain("take out jord");
    expect(canConfirmTokens(v, staged, 1, { planetIds: ["lodor", "jord"], tradeGoods: 0 })).toBe(false);
    expect(tokenOutcome(v, staged, 1, { planetIds: ["jord"], tradeGoods: 0 })).toBeNull();
  });

  it("checks validity again for a different number of bought tokens", () => {
    const v = view();
    const override = { planetIds: ["lodor"], tradeGoods: 0 };
    expect(overrideIsValid(v, 1, override)).toBe(true);
    expect(overrideIsValid(v, 2, override)).toBe(false);
    expect(overrideIsValid(v, 1, { planetIds: ["lodor", "arcturus"], tradeGoods: 0 })).toBe(false);
    expect(overrideIsValid(v, 2, { planetIds: ["lodor", "arcturus"], tradeGoods: 0 })).toBe(true);
  });

  it("sends the chosen exhaust and trade good steps, smallest last, in the order the engine asks", () => {
    const v = view();
    const steps = tokenPlanWithPurchase(v, staged, 1, { planetIds: ["jord"], tradeGoods: 1 });
    expect(steps).toEqual([
      { kind: "purchase", buy: true },
      { kind: "exhaust", planet: "jord" },
      { kind: "trade_good" },
      { kind: "pool", pool: "tactic_tokens" },
      { kind: "purchase", buy: false },
    ]);
    const two = tokenPlanWithPurchase(v, { tactic: 1, fleet: 1, strategic: 0 }, 2, {
      planetIds: ["jord", "arcturus"],
      tradeGoods: 0,
    });
    // 4 covers the first token (1 carried), 2 more covers the second.
    expect(two?.filter((s) => s.kind === "exhaust")).toEqual([
      { kind: "exhaust", planet: "arcturus" },
      { kind: "exhaust", planet: "jord" },
    ]);
    expect(two?.[0]).toEqual({ kind: "purchase", buy: true });
  });

  it("builds the starting override from the Auto-pay plan", () => {
    const plan = planPayment(view(), 1)!;
    expect(overrideIsValid(view(), 1, overrideFromPlan(plan))).toBe(true);
  });
});

describe("restack (Predictive Intelligence)", () => {
  const moveIds = ["tactic|fleet", "tactic|strategy", "fleet|tactic", "fleet|strategy", "strategy|tactic", "strategy|fleet", "done"];
  const restack = () => ({
    options: moveIds.map((id) => ({ id, kind: id === "done" ? "decline" : "redistribute", label: id })),
    details: {
      kind: "command_tokens",
      mode: "restack",
      pools: { tactic: 3, fleet: 4, strategic: 2 },
      reinforcements: 7,
      total: 9,
    },
  });

  it("reads the pools and the total, and needs no batch submitter", () => {
    const view = describeCommandTokens(restack(), false)!;
    expect(view.mode).toBe("restack");
    expect(view.current).toEqual({ tactic: 3, fleet: 4, strategic: 2 });
    expect(view.total).toBe(9);
    expect(view.arrangements.has("tactic|fleet")).toBe(true);
  });

  it("is not offered without any move option", () => {
    const none = { ...restack(), options: [{ id: "done", kind: "decline", label: "done" }] };
    expect(describeCommandTokens(none, true)).toBeNull();
  });

  it("plans the fewest single moves, naming the strategy pool as the engine does", () => {
    const view = describeCommandTokens(restack(), false)!;
    expect(restackMoves(view, { tactic: 3, fleet: 4, strategic: 2 })).toEqual([]);
    expect(restackMoves(view, { tactic: 1, fleet: 4, strategic: 4 })).toEqual([
      "tactic|strategy",
      "tactic|strategy",
    ]);
    expect(restackMoves(view, { tactic: 0, fleet: 3, strategic: 6 })).toEqual([
      "tactic|strategy",
      "tactic|strategy",
      "tactic|strategy",
      "fleet|strategy",
    ]);
  });

  it("finishes with done unless every token was moved (then the engine stops asking itself)", () => {
    const view = describeCommandTokens(restack(), false)!;
    expect(tokenOutcome(view, { tactic: 3, fleet: 4, strategic: 2 })).toEqual({
      kind: "moves",
      moveIds: [],
      finish: true,
    });
    expect(tokenOutcome(view, { tactic: 1, fleet: 4, strategic: 4 })).toEqual({
      kind: "moves",
      moveIds: ["tactic|strategy", "tactic|strategy"],
      finish: true,
    });
    const swap = { ...view, current: { tactic: 3, fleet: 0, strategic: 0 }, total: 3 };
    expect(tokenOutcome(swap, { tactic: 0, fleet: 3, strategic: 0 })).toEqual({
      kind: "moves",
      moveIds: ["tactic|fleet", "tactic|fleet", "tactic|fleet"],
      finish: false,
    });
  });
});

describe("gainTokensPrompt", () => {
  const prompt = "gain a command token into which pool";
  const gain = (tokens: unknown, text = prompt) => ({
    prompt: text,
    details: { kind: "command_tokens", mode: "gain", tokens_to_place: tokens },
  });
  it("names the real count and pluralises", () => {
    expect(gainTokensPrompt(gain(3))).toBe("Gain 3 command tokens — assign them to pools");
    expect(gainTokensPrompt(gain(2))).toBe("Gain 2 command tokens — assign them to pools");
    expect(gainTokensPrompt(gain(1))).toBe("Gain 1 command token — choose its pool");
  });
  it("leaves other prompts, missing counts and redistributions alone", () => {
    expect(gainTokensPrompt(gain(3, "something else"))).toBe("something else");
    expect(gainTokensPrompt(gain(undefined))).toBe(prompt);
    expect(gainTokensPrompt({ prompt })).toBe(prompt);
    expect(
      gainTokensPrompt({
        prompt,
        details: { kind: "command_tokens", mode: "redistribute", total: 9 },
      }),
    ).toBe(prompt);
  });
});
