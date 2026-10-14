import { describe, expect, it } from "vitest";
import { actionToastText } from "./actionToasts.ts";
import { explorationSummary, explorationToasts, readExplorationLog } from "./explorationToasts.ts";

const rec = (over: Record<string, unknown>) => ({
  player: "p2",
  deck: "HAZARDOUS",
  card: "cm1",
  planet: "mehar_xull",
  outcome: "resolved",
  ...over,
});

describe("explorationSummary", () => {
  it("says what a resolved card gave", () => {
    expect(explorationSummary(rec({ result: "gained 1 trade good, removed 1 infantry from the planet" }))).toBe(
      "explored Mehar Xull (hazardous): Core Mine - gained 1 trade good, removed 1 infantry from the planet",
    );
  });
  it("says a declined sacrifice gained nothing", () => {
    expect(explorationSummary(rec({}))).toContain("no infantry removed, nothing gained");
  });
  it("falls back to the structured gains and adds what was placed on the map", () => {
    expect(explorationSummary(rec({ gains: { trade_goods: 2, command_tokens: -1 } }))).toContain(
      "gained 2 trade goods, lost 1 command token",
    );
    expect(
      explorationSummary(rec({ card: "gamma", result: "", placed: ["wormhole:GAMMA:25"] })),
    ).toContain("placed a gamma wormhole in system 25");
    expect(
      explorationSummary(rec({ result: "gained 1 trade good", gains: { trade_goods: 1 }, placed: ["ion_storm:30:ALPHA"] })),
    ).toContain("gained 1 trade good, placed the ion storm in system 30 (alpha side)");
  });
  it("names attachments and fragments", () => {
    expect(explorationSummary(rec({ card: "mw", outcome: "attached:miningworld" }))).toContain(
      "attached Mining World",
    );
    expect(
      explorationSummary(rec({ card: "crf1", deck: "CULTURAL", outcome: "fragment", result: "took a cultural relic fragment" })),
    ).toContain("took a cultural relic fragment");
  });
  it("shows attachment values and says an Enigmatic Device is kept for later", () => {
    expect(explorationSummary(rec({ card: "mw", outcome: "attached:miningworld" }))).toMatch(/attached Mining World/);
    const device = explorationSummary(rec({ card: "ed2", deck: "FRONTIER", planet: null, system: "50", result: "" }));
    expect(device).toContain("Enigmatic Device");
    expect(device).toContain("kept faceup in the play area for later");
  });
  it("covers frontier explorations of systems and empty results", () => {
    const text = explorationSummary(rec({ card: "ent", deck: "FRONTIER", planet: null, system: "67", result: "" }));
    expect(text).toContain("frontier token in system 67");
    expect(text).toContain("nothing gained");
  });
});

describe("explorationToasts", () => {
  it("toasts only records after the cursor and skips unfinished ones", () => {
    const log = [rec({ result: "gained 1 trade good" }), rec({ card: "cm2", result: "gained 1 trade good" }), rec({ outcome: "" })];
    const first = explorationToasts(log, 1);
    expect(first.toasts).toHaveLength(1);
    expect(first.toasts[0]).toMatchObject({ actor: "p2", category: "explore" });
    expect(first.next).toBe(2);
    expect(explorationToasts(log, 5).toasts).toEqual([]);
  });
  it("is not held back by an older record that never got an outcome", () => {
    const log = [rec({ outcome: "" }), rec({ card: "cm2", result: "gained 1 trade good" })];
    const { toasts, next } = explorationToasts(log, 0);
    expect(toasts).toHaveLength(1);
    expect(next).toBe(2);
  });
  it("joins merged exploration toasts", () => {
    expect(actionToastText({ id: "x", actor: "p1", category: "explore", count: 2, parts: ["a", "b"] })).toBe("a; b");
  });
  it("reads a missing log as empty", () => {
    expect(readExplorationLog({})).toEqual([]);
  });
});
