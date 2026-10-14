import { describe, expect, it } from "vitest";
import { describeTradeReplenish } from "./tradeReplenish.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";

const choice = (details: Record<string, unknown> | undefined): PendingChoiceDto => ({
  prompt: "let another player replenish commodities",
  actor: "a",
  nonce: "n",
  context: { subtype: "trade_choose_replenish" } as never,
  options: [
    { id: "hacan", kind: "replenish", label: "hacan replenishes commodities" },
    { id: "sol", kind: "replenish", label: "sol replenishes commodities" },
    { id: "done", kind: "decline", label: "nobody else replenishes" },
  ],
  details,
});

const details = {
  seats: { hacan: "seat_b", sol: "seat_c" },
  commodities: { hacan: { have: 1, max: 6 }, sol: { have: 0, max: 4 } },
};

describe("describeTradeReplenish", () => {
  it("lists each seat with its commodities and the gain", () => {
    const view = describeTradeReplenish(choice(details));
    expect(view?.done.id).toBe("done");
    expect(view?.rows.map((r) => [r.option.id, r.seat, r.have, r.max, r.gain])).toEqual([
      ["hacan", "seat_b", 1, 6, 5],
      ["sol", "seat_c", 0, 4, 4],
    ]);
  });

  it("is null without commodity figures, a done option, or the right subtype", () => {
    expect(describeTradeReplenish(choice(undefined))).toBeNull();
    expect(describeTradeReplenish({ ...choice(details), context: { subtype: "x" } as never })).toBeNull();
    expect(
      describeTradeReplenish({ ...choice(details), options: choice(details).options.slice(0, 2) }),
    ).toBeNull();
    expect(
      describeTradeReplenish(choice({ ...details, commodities: { hacan: { have: 1, max: 6 } } })),
    ).toBeNull();
  });
});
