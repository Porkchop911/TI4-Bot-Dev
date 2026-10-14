import { describe, expect, it } from "vitest";
import {
  pickInfluencePlanets,
  suggestAutoPay,
  type PayablePlanet,
  type PaymentOffer,
} from "./paymentDraft.ts";

const planet = (id: string, worth: number, resources?: number): PayablePlanet => ({
  id: `exhaust|${id}`,
  planetId: id,
  planetName: id,
  worth,
  label: id,
  resources,
});

const offerOf = (
  planets: PayablePlanet[],
  owed: number,
  currency: "Influence" | "Resources" = "Influence",
  tradeGoodWorth = 1,
): PaymentOffer => ({
  planets,
  hasTradeGoodOption: true,
  tradeGoodWorth,
  owed,
  totalAmount: owed,
  alreadyPaid: 0,
  currency,
});

const names = (ids: string[]) => ids.map((id) => id.replace("exhaust|", "")).sort();

describe("influence Auto-pay policy: planets with no resources first, then do not waste resources", () => {
  it("uses the zero-resource planets and leaves a resource planet that would fit exactly", () => {
    // 3 influence: centauri (3 influence, 1 resource) is exact, but dal (2, 0) + rarron (2, 0) waste 1 and cost no resource.
    const offer = offerOf([planet("centauri", 3, 1), planet("dal", 2, 0), planet("rarron", 2, 0)], 3);
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["dal", "rarron"]);
  });

  it("among zero-resource planets prefers the least waste, then the fewest planets", () => {
    const offer = offerOf([planet("a", 4, 0), planet("b", 2, 0), planet("c", 1, 0), planet("d", 2, 1)], 3);
    // b + c = 3 exact with two planets beats a (waste 1).
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["b", "c"]);
  });

  it("when zero-resource planets cannot cover the bill, adds the resource planets that cost the fewest resources", () => {
    const offer = offerOf(
      [planet("pure", 1, 0), planet("rich", 3, 3), planet("poor", 3, 1), planet("mid", 2, 2)],
      4,
    );
    // pure (1, 0) + poor (3, 1) = 4 exact for 1 resource; pure + rich costs 3; mid + ... cannot beat that.
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["poor", "pure"]);
  });

  it("minimises resources before waste: a wasteful low-resource set beats an exact high-resource one", () => {
    const offer = offerOf([planet("exact", 3, 2), planet("big", 5, 1)], 3);
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["big"]);
  });

  it("with equal resources, wastes the least influence", () => {
    const offer = offerOf([planet("x", 4, 1), planet("y", 3, 1)], 3);
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["y"]);
  });

  it("with equal resources and waste, uses the fewest planets, then the smallest planet id", () => {
    const fewer = offerOf([planet("a", 1, 0), planet("b", 2, 0), planet("c", 3, 0)], 3);
    // c alone (waste 0, 1 planet) beats a + b (waste 0, 2 planets).
    expect(names(suggestAutoPay(fewer, 0).planetIds)).toEqual(["c"]);
    const tie = offerOf([planet("zeta", 3, 0), planet("alpha", 3, 0)], 3);
    expect(names(suggestAutoPay(tie, 0).planetIds)).toEqual(["alpha"]);
  });

  it("is deterministic regardless of the order the planets are listed in", () => {
    const list = [planet("a", 2, 0), planet("b", 2, 0), planet("c", 2, 0), planet("d", 3, 1)];
    const forward = suggestAutoPay(offerOf(list, 4), 0).planetIds;
    const backward = suggestAutoPay(offerOf([...list].reverse(), 4), 0).planetIds;
    expect(names(forward)).toEqual(names(backward));
    expect(names(forward)).toEqual(["a", "b"]);
  });

  it("without any zero-resource planet it still minimises the exhausted resources", () => {
    const offer = offerOf([planet("a", 2, 3), planet("b", 2, 1), planet("c", 3, 2), planet("d", 1, 1)], 3);
    // b + d = 3 influence for 2 resources; c alone is 3 for 2 resources too but one planet wins the tie at equal waste.
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["c"]);
  });

  it("unknown resources count as 0 and reduce the policy to least waste, then fewest planets", () => {
    const offer = offerOf([planet("a", 4), planet("b", 3), planet("c", 2), planet("d", 1)], 3);
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["b"]);
  });

  it("keeps the trade goods rule: only when the planets alone cannot cover the bill", () => {
    const offer = offerOf([planet("a", 2, 0), planet("b", 1, 2)], 5);
    expect(suggestAutoPay(offer, 4)).toMatchObject({ tradeGoods: 2, settled: true });
    expect(suggestAutoPay(offer, 1)).toMatchObject({ tradeGoods: 1, settled: false });
    const covered = offerOf([planet("a", 2, 0), planet("b", 3, 2)], 5);
    expect(suggestAutoPay(covered, 4).tradeGoods).toBe(0);
  });

  it("does not change resource payments: they stay least waste, then fewest planets", () => {
    const planets = [planet("centauri", 3, 1), planet("dal", 2, 0), planet("rarron", 2, 0)];
    const offer = offerOf(planets, 3, "Resources");
    expect(names(suggestAutoPay(offer, 0).planetIds)).toEqual(["centauri"]);
  });

  it("returns null from the pure picker when the planets cannot cover the bill", () => {
    expect(pickInfluencePlanets([planet("a", 1, 0)], 3)).toBeNull();
  });
});
