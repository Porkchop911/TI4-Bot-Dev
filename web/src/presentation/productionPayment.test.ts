import { describe, expect, it } from "vitest";
import type { PendingChoiceDto } from "../protocol/types.ts";
import { derivePaymentOffer, suggestAutoPay, type PaymentDraft } from "./paymentDraft.ts";
import {
  allocateQuestion,
  autoPaymentFor,
  buildCosts,
  isProductionPayQuestion,
  newProductionPay,
  planPanel,
  unsentBuilds,
  type ProductionPay,
} from "./productionPayment.ts";

const build = (unit: string, cost: number, extra: Record<string, unknown> = {}) => ({
  id: `build|${unit}|1`,
  kind: "produce",
  label: unit,
  payload: { unit, cost, printed_cost: cost, ...extra },
});

const produceChoice = (): PendingChoiceDto => ({
  actor: "p1",
  nonce: "n",
  prompt: "produce",
  context: { subtype: "produce_unit", target: { System: "18" } },
  options: [build("destroyer", 2), build("cruiser", 3), build("fighter", 1, { free_this_use: true }), { id: "decline", kind: "decline", label: "Done" }],
});

const planet = (id: string, worth: number, source?: string) => ({
  id: `exhaust|${id}`,
  kind: "pay",
  label: id,
  payload: { worth, planet_name: id, ...(source ? { source } : {}) },
});

const question = (
  owed: number,
  planets: ReturnType<typeof planet>[],
  tradeGoods = true,
  subtype = "pay_resources",
): PendingChoiceDto => ({
  actor: "p1",
  nonce: `q${owed}`,
  prompt: `Pay ${owed}`,
  context: {
    subtype,
    target: { System: "18" },
    outstanding: [{ kind: "Resources", amount: owed, paid: 0 }],
  },
  options: [
    ...planets,
    ...(tradeGoods ? [{ id: "trade_good", kind: "pay", label: "Trade good", payload: { worth: 1 } }] : []),
  ],
});

const planFor = (units: string[]): ProductionPay => newProductionPay(produceChoice(), units)!;

describe("production payment plan", () => {
  it("prices each staged build from the production question, free builds at 0", () => {
    expect(buildCosts(produceChoice(), ["destroyer", "cruiser", "fighter"])).toEqual([2, 3, 0]);
    expect(buildCosts(produceChoice(), ["dreadnought"])).toBeNull();
  });

  it("makes a plan only for several builds", () => {
    expect(newProductionPay(produceChoice(), ["destroyer"])).toBeNull();
    expect(planFor(["destroyer", "destroyer"]).mode).toBe("ask");
  });

  it.each([
    [["destroyer", "destroyer"], 2, 4, 2],
    [["destroyer", "destroyer", "cruiser"], 2, 7, 3],
    [["cruiser", "destroyer", "destroyer"], 3, 7, 3],
  ])("the one panel for %j totals the open bill plus the unsent builds", (units, open, total, builds) => {
    const plan = { ...planFor(units), submitted: 1 };
    const base = derivePaymentOffer(question(open, [planet("jord", 3)]));
    const panel = planPanel(question(open, [planet("jord", 3)]), base, plan);
    expect(panel).not.toBeNull();
    expect(panel!.offer.owed).toBe(total);
    expect(panel!.builds).toBe(builds);
  });

  it("has no panel for the last build (a plain single payment) or once the plan is running", () => {
    const q = question(2, [planet("jord", 3)]);
    const base = derivePaymentOffer(q);
    expect(planPanel(q, base, { ...planFor(["destroyer", "destroyer"]), submitted: 2 })).toBeNull();
    expect(planPanel(q, base, { ...planFor(["destroyer", "destroyer"]), submitted: 1, mode: "auto" })).toBeNull();
    expect(planPanel(q, base, null)).toBeNull();
    expect(unsentBuilds({ ...planFor(["destroyer", "destroyer", "destroyer"]), submitted: 1 })).toEqual({
      count: 2,
      cost: 4,
    });
  });

  it("recognises only this production's resource payment question", () => {
    const plan = planFor(["destroyer", "destroyer"]);
    expect(isProductionPayQuestion(question(2, [planet("jord", 3)]), plan)).toBe(true);
    expect(isProductionPayQuestion(question(2, [planet("jord", 3)], true, "pay_influence"), plan)).toBe(false);
    expect(isProductionPayQuestion({ ...question(2, [planet("jord", 3)]), actor: "p2" }, plan)).toBe(false);
    const elsewhere = question(2, [planet("jord", 3)]);
    elsewhere.context = { ...elsewhere.context!, target: { System: "19" } };
    expect(isProductionPayQuestion(elsewhere, plan)).toBe(false);
  });
});

describe("allocating one plan over the engine's payment questions", () => {
  const planets = [planet("jord", 3), planet("vega", 3), planet("lodor", 2)];

  it("spends planets in plan order until the open bill is covered and keeps the rest", () => {
    const offer = derivePaymentOffer(question(2, planets));
    const plan: PaymentDraft = { planetIds: ["exhaust|jord", "exhaust|vega"], tradeGoods: 0 };
    const first = allocateQuestion(offer, plan, 0)!;
    expect(first.draft).toEqual({ planetIds: ["exhaust|jord"], tradeGoods: 0 });
    expect(first.rest).toEqual({ planetIds: ["exhaust|vega"], tradeGoods: 0 });
  });

  it("walks 3 builds of cost 2 with overpayment carried as credit", () => {
    // The plan the panel prefills for a bill of 6 over these planets.
    const plan = suggestAutoPay(derivePaymentOffer(question(6, planets)), 0);
    expect(plan.settled).toBe(true);
    let rest: PaymentDraft = { planetIds: plan.planetIds, tradeGoods: plan.tradeGoods };
    // Build 1 owes 2 and is paid by a 3-resource planet: credit 1. Build 2 owes 2 - 1 = 1.
    const first = allocateQuestion(derivePaymentOffer(question(2, planets)), rest, 0)!;
    rest = first.rest;
    const second = allocateQuestion(derivePaymentOffer(question(1, planets)), rest, 0)!;
    rest = second.rest;
    // Build 3 owes 2 - 2 (credit) = nothing: the engine does not ask, and the plan keeps nothing unneeded.
    expect(first.draft.planetIds).toHaveLength(1);
    expect(second.draft.planetIds).toHaveLength(1);
    expect(rest.planetIds).toEqual([]);
    expect(new Set([...first.draft.planetIds, ...second.draft.planetIds]).size).toBe(2);
  });

  it("covers what planets leave with trade goods, and refuses when they are not there", () => {
    const offer = derivePaymentOffer(question(4, [planet("jord", 3)]));
    const plan: PaymentDraft = { planetIds: ["exhaust|jord"], tradeGoods: 1 };
    expect(allocateQuestion(offer, plan, 2)).toEqual({
      draft: { planetIds: ["exhaust|jord"], tradeGoods: 1 },
      rest: { planetIds: [], tradeGoods: 0 },
    });
    expect(allocateQuestion(offer, plan, 0)).toBeNull();
    expect(allocateQuestion(offer, { planetIds: ["exhaust|jord"], tradeGoods: 0 }, 5)).toBeNull();
  });

  it("uses trade goods alone when the plan has no planets", () => {
    const offer = derivePaymentOffer(question(2, []));
    expect(allocateQuestion(offer, { planetIds: [], tradeGoods: 3 }, 3)).toEqual({
      draft: { planetIds: [], tradeGoods: 2 },
      rest: { planetIds: [], tradeGoods: 1 },
    });
  });

  it("refuses a plan whose planet is no longer offered", () => {
    const offer = derivePaymentOffer(question(1, [planet("lodor", 2)]));
    expect(allocateQuestion(offer, { planetIds: ["exhaust|vega"], tradeGoods: 0 }, 0)).toBeNull();
  });

  it("matches a planet offered in another variant by the planet, not the option id", () => {
    const offer = derivePaymentOffer(question(2, [planet("jord", 3, "influence")]));
    const result = allocateQuestion(offer, { planetIds: ["exhaust|jord|resources"], tradeGoods: 0 }, 0);
    expect(result?.draft.planetIds).toEqual(["exhaust|jord"]);
  });

  it("builds the payment batch steps for the open question and describes them", () => {
    const q = question(2, planets);
    const auto = autoPaymentFor(q, { planetIds: ["exhaust|jord", "exhaust|vega"], tradeGoods: 0 }, 0)!;
    expect(auto.steps).toEqual([{ kind: "exhaust", planet: "jord" }]);
    expect(auto.rest.planetIds).toEqual(["exhaust|vega"]);
    expect(auto.summary).toBe("jord");
    expect(autoPaymentFor(q, { planetIds: [], tradeGoods: 0 }, 0)).toBeNull();
  });
});
