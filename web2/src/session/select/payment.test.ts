import { expect, it } from "vitest";
import type { SessionUpdate } from "../wire";
import {
  COMMODITY,
  NO_PAYMENT,
  autoPaying,
  goodsPaying,
  paidBy,
  payOfferOf,
  paySteps,
  paymentOf,
} from "./payment";

// The details of a payment of the engine: Archon's Gift pays Mecatol Rex with its other value,
// and the Keleres agent lets two commodities pay as trade goods.
const offer = payOfferOf({
  cost: 3,
  planets: [
    { id: "jord", worth: 4 },
    { id: "mr", worth: 6, face: "influence" },
  ],
  trade_goods: 1,
  trade_good_worth: 1,
  commodities: 2,
});

const update = {
  view: {
    board: {
      map_tiles: [
        { system_id: "1", planets: [{ id: "jord", label: "Jord" }] },
        { system_id: "18", planets: [{ id: "mr", label: "Mecatol Rex" }] },
      ],
    },
  },
} as unknown as SessionUpdate;

it("reads the face of a planet and the commodities from the details", () => {
  expect(offer.planets[1]).toEqual({ id: "mr", worth: 6, face: "influence" });
  expect(offer.commodities).toBe(2);
});

it("stages commodities on their own counter and sends each payment as the game names it", () => {
  const paying = goodsPaying(offer, { ...NO_PAYMENT, planets: ["mr"] }, 5, COMMODITY);
  expect(paying.commodities).toBe(2);
  expect(paidBy(offer, paying)).toBe(8);
  expect(paySteps(offer, paying)).toEqual([
    { kind: "pay", source: "exhaust|mr|influence" },
    { kind: "pay", source: "commodity" },
    { kind: "pay", source: "commodity" },
  ]);
  const { payment, task } = paymentOf(update, offer, paying, 8, "resources");
  // The map shows the value that pays large: the influence of Mecatol Rex.
  expect(task.faces).toEqual({ mr: "influence" });
  expect(payment.summary).toBe("Mecatol Rex 6 influence + 2 commodities");
  expect(payment.commodities).toEqual({ id: COMMODITY, value: 2, max: 2 });
});

it("auto-pays with commodities before trade goods", () => {
  expect(autoPaying(offer, 5)).toEqual({ planets: ["jord"], goods: 0, commodities: 1 });
});
