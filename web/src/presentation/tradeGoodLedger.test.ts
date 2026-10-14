import { describe, it, expect } from "vitest";
import type { GameLogEntry } from "../protocol/client.ts";
import type { PlayerView } from "../protocol/types.ts";
import { LEDGER_LIMIT, recordTradeGoodChanges, sourceOfChange } from "./tradeGoodLedger.ts";

const player = (id: string, trade_goods: number) => ({ id, trade_goods }) as PlayerView;
const entry = (id: string, detail?: string, round = 1): GameLogEntry =>
  ({ id, detail, round, stage: "effects", visibility: { visibility: "public" }, event: { kind: "decision_resolved" } }) as GameLogEntry;

describe("trade good ledger", () => {
  it("records the change with the log line that arrived with it", () => {
    const events = [entry("a", "old"), entry("b", "p1 resolved trade"), entry("c")];
    const ledger = recordTradeGoodChanges({}, { p1: 4, p2: 1 }, [player("p1", 7), player("p2", 1)], events, "a");
    expect(ledger.p2).toBeUndefined();
    expect(ledger.p1).toEqual([{ seat: "p1", delta: 3, balance: 7, round: 1, source: "p1 resolved trade" }]);
  });

  it("records spending as a negative change and keeps the newest first", () => {
    const first = recordTradeGoodChanges({}, { p1: 2 }, [player("p1", 5)], [entry("a", "gain")], null);
    const second = recordTradeGoodChanges(first, { p1: 5 }, [player("p1", 3)], [entry("a", "gain"), entry("b", "paid")], "a");
    expect(second.p1.map((c) => c.delta)).toEqual([-2, 3]);
  });

  it("does not count a seat's first appearance as a gain", () => {
    expect(recordTradeGoodChanges({}, {}, [player("p1", 7)], [], null)).toEqual({});
  });

  it("caps the list", () => {
    let ledger = {};
    for (let n = 0; n < LEDGER_LIMIT + 3; n++)
      ledger = recordTradeGoodChanges(ledger, { p1: n }, [player("p1", n + 1)], [], null);
    expect((ledger as Record<string, unknown[]>).p1).toHaveLength(LEDGER_LIMIT);
  });

  it("falls back to the stage when no new entry has text", () => {
    expect(sourceOfChange([entry("a")], null).source).toBe("effects");
    expect(sourceOfChange([], null).source).toBeNull();
  });
});
