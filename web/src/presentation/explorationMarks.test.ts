import { describe, expect, it } from "vitest";
import { attachmentLabels, explorationsInSystem, readFrontierTokens } from "./explorationMarks.ts";

const rec = (over: Record<string, unknown>) => ({
  round: 1,
  player: "p1",
  deck: "INDUSTRIAL",
  card: "aw2",
  planet: "jeolir",
  outcome: "resolved",
  result: "gained 2 commodities",
  ...over,
});

describe("explorationsInSystem", () => {
  it("lists a system's planet explorations and its frontier token explorations, newest last", () => {
    const log = [
      rec({}),
      rec({ planet: "elsewhere", card: "cm1" }),
      rec({ planet: null, system: "50", deck: "FRONTIER", card: "ed2", result: "" }),
      rec({ outcome: "", card: "cm2" }),
    ];
    const here = explorationsInSystem(log, "50", ["jeolir"]);
    expect(here.map((e) => e.card)).toEqual(["Abandoned Warehouses", "Enigmatic Device"]);
    expect(here[0]).toMatchObject({ kind: "gain", result: "gained 2 commodities", where: "Jeol Ir (industrial)" });
    expect(here[1]).toMatchObject({ kind: "frontier", where: "frontier token in system 50" });
  });
  it("marks attachments and fragments", () => {
    const [a, f] = explorationsInSystem(
      [rec({ outcome: "attached:dysonsphere", card: "ds", result: "" }), rec({ outcome: "fragment", card: "irf1", result: "took a industrial relic fragment" })],
      "9",
      ["jeolir"],
    );
    expect(a.kind).toBe("attachment");
    expect(f.kind).toBe("fragment");
  });
});

describe("attachmentLabels", () => {
  it("names attachments with the values they add", () => {
    expect(attachmentLabels(["dysonsphere"])[0]).toContain("Dyson Sphere");
    expect(attachmentLabels(["nonsense_id"])).toEqual(["Nonsense Id"]);
  });
});

describe("readFrontierTokens", () => {
  it("reads the state's token list", () => {
    expect(readFrontierTokens({ frontier_tokens: ["39", "40"] })).toEqual(["39", "40"]);
    expect(readFrontierTokens(null)).toEqual([]);
  });
});
