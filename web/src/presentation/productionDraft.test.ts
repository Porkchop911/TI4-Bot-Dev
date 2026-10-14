import { describe, expect, it } from "vitest";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { buildCosts } from "./productionPayment.ts";
import { optionUnit, planBuilds, produceStep, stepUnitKey } from "./productionDraft.ts";

const build: ChoiceOptionDto = {
  id: "build|carrier|1",
  kind: "produce",
  label: "produce 1x carrier for 3",
  payload: { unit: "carrier", count: 1, cost: 3, printed_cost: 3, discount: 0, production_spent: 1, available_resources: 6 },
};
const exchange: ChoiceOptionDto = {
  id: "exchange|carrier",
  kind: "produce",
  label: "produce 1x carrier by returning a captured carrier",
  payload: { unit: "carrier", count: 1, cost: 0, printed_cost: 3, discount: 0, production_spent: 1, available_resources: 6, exchange: true },
};
const choice = {
  actor: "a",
  nonce: "n",
  prompt: "produce",
  context: { subtype: "produce_unit", target: { System: "14" }, outstanding: [{ amount: 3, paid: 0 }] },
  options: [build, exchange, { id: "done_producing", kind: "decline", label: "done", payload: {} }],
} as unknown as PendingChoiceDto;

describe("exchange vs paid build of one unit type", () => {
  it("keys them differently and sends the flag only for the exchange", () => {
    expect(optionUnit(build)).toBe("carrier");
    expect(optionUnit(exchange)).toBe("exchange|carrier");
    expect(produceStep(build)).toEqual({ kind: "produce", unit: "carrier", count: 1 });
    expect(produceStep(exchange)).toEqual({ kind: "produce", unit: "carrier", count: 1, exchange: true });
    expect(stepUnitKey(produceStep(exchange))).toBe("exchange|carrier");
    expect(stepUnitKey(produceStep(build))).toBe("carrier");
  });
  it("plans a prepared build of either kind without calling it ambiguous", () => {
    const paid = planBuilds(choice, ["carrier"]);
    expect(paid.ok && paid.draft).toEqual({ "build|carrier|1": 1 });
    const swapped = planBuilds(choice, ["exchange|carrier"]);
    expect(swapped.ok && swapped.steps).toEqual([{ kind: "produce", unit: "carrier", count: 1, exchange: true }]);
  });
  it("costs an exchange as nothing", () => {
    expect(buildCosts(choice, ["carrier", "exchange|carrier"])).toEqual([3, 0]);
  });
});
