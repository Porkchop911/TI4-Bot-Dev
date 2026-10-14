import { describe, it, expect } from "vitest";
import type { PendingChoiceDto } from "../protocol/types.ts";
import {
  buildPaymentSteps,
  derivePaymentMarks,
  derivePaymentOffer,
  isPaymentChoice,
  paymentOptionForPlanet,
  paymentProblem,
  suggestAutoPay,
  summarizePayment,
  togglePlanetInDraft,
} from "./paymentDraft.ts";

const planetOpt = (planet: string, worth: number, extra: Record<string, unknown> = {}) => ({
  id: `exhaust|${planet}`,
  label: `Exhaust ${planet}`,
  kind: "pay",
  payload: { worth, owed: 5, kind: "resources", planet_name: planet, ...extra },
});

const choice = (
  owed: number,
  planets: [string, number][],
  opts: { tg?: boolean; subtype?: string } = {},
): PendingChoiceDto => ({
  prompt: `pay ${owed}`,
  actor: "p1",
  nonce: "n1",
  context: {
    subtype: opts.subtype ?? "pay_resources",
    outstanding: [{ kind: "resources", amount: owed, paid: 0 }],
  },
  options: [
    ...planets.map(([p, w]) => planetOpt(p, w)),
    ...(opts.tg ? [{ id: "trade_good", label: "tg", kind: "pay", payload: { worth: 1 } }] : []),
    { id: "decline", label: "Cancel", kind: "decline" },
  ],
});

describe("paymentDraft", () => {
  it("derives the offer: planets, trade goods, owed and currency", () => {
    const offer = derivePaymentOffer(choice(5, [["jord", 4], ["lodor", 3]], { tg: true }));
    expect(offer.owed).toBe(5);
    expect(offer.currency).toBe("Resources");
    expect(offer.hasTradeGoodOption).toBe(true);
    expect(offer.planets.map((p) => [p.planetId, p.worth])).toEqual([["jord", 4], ["lodor", 3]]);
  });

  it("recognises payment decisions but not planet picks or votes", () => {
    expect(isPaymentChoice(choice(2, [["jord", 4]]))).toBe(true);
    expect(isPaymentChoice(choice(2, [["jord", 4]], { subtype: "vote_exhaust_planet" }))).toBe(false);
    expect(isPaymentChoice(null)).toBe(false);
  });

  it("allows one variant per planet when toggling", () => {
    let ids = togglePlanetInDraft([], "exhaust|jord");
    ids = togglePlanetInDraft(ids, "exhaust|jord|influence");
    expect(ids).toEqual(["exhaust|jord|influence"]);
    expect(togglePlanetInDraft(ids, "exhaust|jord|influence")).toEqual([]);
  });

  it("summarises paid against owed, overpayment and settledness", () => {
    const offer = derivePaymentOffer(choice(5, [["jord", 4], ["lodor", 3]], { tg: true }));
    expect(summarizePayment(offer, { planetIds: ["exhaust|jord"], tradeGoods: 0 })).toMatchObject({
      committed: 4,
      shortfall: 1,
      settled: false,
    });
    expect(
      summarizePayment(offer, { planetIds: ["exhaust|jord", "exhaust|lodor"], tradeGoods: 0 }),
    ).toMatchObject({ committed: 7, surplus: 2, settled: true });
  });

  it("explains why the payment cannot be confirmed", () => {
    const offer = derivePaymentOffer(choice(5, [["jord", 4], ["lodor", 3]]));
    expect(paymentProblem(offer, { planetIds: ["exhaust|jord"], tradeGoods: 0 }, 0)).toMatch(/Short by 1/);
    expect(paymentProblem(offer, { planetIds: ["exhaust|jord", "exhaust|lodor"], tradeGoods: 0 }, 0)).toBeNull();
    const poor = derivePaymentOffer(choice(9, [["jord", 4], ["lodor", 3]]));
    expect(paymentProblem(poor, { planetIds: [], tradeGoods: 0 }, 0)).toMatch(/Not enough.*7 of 9/);
  });

  describe("suggestAutoPay", () => {
    it("picks the exact match instead of the biggest planet", () => {
      const offer = derivePaymentOffer(choice(5, [["jord", 4], ["lodor", 3], ["quann", 2], ["x", 1]]));
      const s = suggestAutoPay(offer, 0);
      expect(s.settled).toBe(true);
      const total = s.planetIds.reduce(
        (n, id) => n + (offer.planets.find((p) => p.id === id)?.worth ?? 0),
        0,
      );
      expect(total).toBe(5);
      expect(s.tradeGoods).toBe(0);
    });

    it("minimises waste, then the number of planets", () => {
      const offer = derivePaymentOffer(choice(3, [["a", 4], ["b", 1], ["c", 2], ["d", 1]]));
      const s = suggestAutoPay(offer, 5);
      expect(s.planetIds.sort()).toEqual(["exhaust|b", "exhaust|c"]);
    });

    it("tops up with trade goods only when planets fall short", () => {
      const offer = derivePaymentOffer(choice(9, [["jord", 4], ["lodor", 3]], { tg: true }));
      const s = suggestAutoPay(offer, 5);
      expect(s).toMatchObject({ tradeGoods: 2, settled: true });
      expect(s.planetIds).toHaveLength(2);
    });

    it("reports an unsettled best effort when the bill cannot be covered", () => {
      const offer = derivePaymentOffer(choice(9, [["jord", 4]], { tg: true }));
      expect(suggestAutoPay(offer, 2)).toMatchObject({ tradeGoods: 2, settled: false });
    });

    it("never stages two variants of one planet", () => {
      const c = choice(3, [["jord", 3]]);
      c.options.push({
        id: "exhaust|jord|influence",
        label: "x",
        kind: "pay",
        payload: { worth: 3, kind: "resources", source: "influence", planet_name: "Jord" },
      });
      const s = suggestAutoPay(derivePaymentOffer(c), 0);
      expect(s.planetIds).toEqual(["exhaust|jord"]);
    });
  });

  it("builds map marks with the staged state and picks a map click's option", () => {
    const offer = derivePaymentOffer(choice(5, [["jord", 4], ["lodor", 3]]));
    const marks = derivePaymentMarks(offer, { planetIds: ["exhaust|jord"], tradeGoods: 0 });
    expect(marks.get("jord")).toMatchObject({ worth: 4, unit: "R", staged: true });
    expect(marks.get("lodor")).toMatchObject({ staged: false, optionId: "exhaust|lodor" });
    expect(paymentOptionForPlanet(offer, "lodor")?.id).toBe("exhaust|lodor");
    expect(paymentOptionForPlanet(offer, "nope")).toBeNull();
  });

  it("builds the basket steps without spends the bill does not need", () => {
    const c = choice(4, [["jord", 4], ["lodor", 3]], { tg: true });
    const offer = derivePaymentOffer(c);
    expect(
      buildPaymentSteps(c, offer, { planetIds: ["exhaust|jord", "exhaust|lodor"], tradeGoods: 2 }),
    ).toEqual([{ kind: "exhaust", planet: "jord" }]);
    const c2 = choice(6, [["jord", 4], ["lodor", 3]], { tg: true });
    expect(
      buildPaymentSteps(c2, derivePaymentOffer(c2), { planetIds: ["exhaust|jord"], tradeGoods: 2 }),
    ).toEqual([{ kind: "exhaust", planet: "jord" }, { kind: "trade_good" }, { kind: "trade_good" }]);
  });
});
