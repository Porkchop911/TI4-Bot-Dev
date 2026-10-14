import { describe, expect, it } from "vitest";
import {
  breakthroughNote,
  breakthroughToasts,
  describeBreakthrough,
  readBreakthroughs,
} from "./breakthroughs.ts";
import { BREAKTHROUGHS, findBreakthroughForFaction } from "../protocol/contentCatalog.ts";
import { describeFaction } from "./factionInfo.ts";
import { decisionBreakthrough, describeDecisionHeader } from "./decisionSource.ts";
import { describeComponent } from "./componentInfo.ts";

describe("breakthrough catalog", () => {
  it("has all 31 Thunder's Edge breakthroughs, each with text and a synergy", () => {
    const cards = Object.values(BREAKTHROUGHS);
    expect(cards).toHaveLength(31);
    for (const card of cards) {
      expect(card.text, card.id).toBeTruthy();
      expect(card.synergy.length, card.id).toBeGreaterThan(0);
    }
  });
  it("finds a faction's card by id or printed name", () => {
    expect(findBreakthroughForFaction("sol")?.id).toBe("solbt");
    expect(findBreakthroughForFaction("Federation of Sol")?.id).toBe("solbt");
    expect(findBreakthroughForFaction("nobody")).toBeUndefined();
  });
});

describe("describeBreakthrough", () => {
  it("shows a locked card with its public text, and an unlocked one", () => {
    expect(describeBreakthrough("sol", null)).toMatchObject({ name: "Bellum Gloriosum", unlocked: false });
    const held = describeBreakthrough("sol", "solbt");
    expect(held?.unlocked).toBe(true);
    expect(held?.synergy).toContain("+");
    expect(held?.text).toBeTruthy();
  });
  it("returns nothing for a faction without a card", () => {
    expect(describeBreakthrough("nobody", null)).toBeNull();
  });
});

describe("readBreakthroughs and toasts", () => {
  const state = (xxcha: string | null) => ({ players: [{ id: "p1", breakthrough: null }, { id: "p2", breakthrough: xxcha }] });
  it("reads each seat's earned alias", () => {
    expect(readBreakthroughs(state("xxchabt"))).toEqual({ p1: null, p2: "xxchabt" });
    expect(readBreakthroughs(undefined)).toEqual({});
  });
  it("toasts only a newly unlocked breakthrough, never what was there at load", () => {
    const before = readBreakthroughs(state(null));
    const after = readBreakthroughs(state("xxchabt"));
    expect(breakthroughToasts(null, after)).toEqual([]);
    const toasts = breakthroughToasts(before, after);
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toMatchObject({ actor: "p2", category: "breakthrough" });
    expect(toasts[0].parts[0]).toContain("Archon's Gift");
    expect(breakthroughToasts(after, after)).toEqual([]);
  });
});

describe("where the card text shows up", () => {
  it("is part of the faction card", () => {
    const info = describeFaction("sol");
    expect(info?.breakthrough).toMatchObject({ name: "Bellum Gloriosum" });
    expect(info?.breakthrough?.text).toBeTruthy();
  });
  it("is attached to the expedition slice that grants it", () => {
    const option = { id: "component|expedition|arch", kind: "component", label: "Thunder's Edge expedition: x" };
    const description = describeComponent(option as never, "sol");
    expect(description.note).toContain("Bellum Gloriosum");
    expect(breakthroughNote("sol")).toContain("synergy");
    expect(describeComponent(option as never).note).toBeNull();
  });
  it("names the card on a decision it raised", () => {
    const choice = { context: { subtype: "yssaril_scheming", source: { Content: "yssarilbt" } } } as never;
    expect(decisionBreakthrough(choice)?.name).toBe("Deepgloom Executable");
    expect(describeDecisionHeader(choice).eyebrow).toContain("Deepgloom Executable");
    expect(decisionBreakthrough({ context: { subtype: "x", source: { Rule: "37.3" } } } as never)).toBeUndefined();
  });
});
