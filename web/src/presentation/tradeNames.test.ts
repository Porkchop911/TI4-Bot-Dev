import { describe, it, expect } from "vitest";
import type { ChoiceOptionDto } from "../protocol/types.ts";
import { decodeTradeOption } from "./tradeDecoder.ts";
import { dealToStaged, sideLines, emptySide } from "./tradeStaging.ts";
import { humanizeOfferPrompt, printedCardName, printedItemLabel } from "./tradeNames.ts";

// Morning analysis 2026-10-07 finding 7: raw ids in sideLines, Quick deals and the answer prompt.
describe("printedCardName", () => {
  it("names generic and faction notes", () => {
    expect(printedCardName("note", "cf:generic")).toEqual({ name: "Ceasefire", known: true });
    expect(printedCardName("note", "ra:jolnar").name).toBe("Research Agreement (jolnar)");
    expect(printedCardName("note", "support_for_throne").name).toBe("Support for the Throne");
  });
  it("names action cards and secret objectives", () => {
    expect(printedCardName("action", "sabo1").name).toBe("Sabotage");
    expect(printedCardName("secret", "ans").name).toBe("Adapt New Strategies");
  });
  it("falls back to readable text for unknown ids", () => {
    expect(printedCardName("note", "mystery_pact:generic")).toEqual({ name: "Mystery Pact", known: false });
    expect(printedCardName("note", "mystery_pact:sol").name).toBe("Mystery Pact (sol)");
    expect(printedCardName("action", "nope_card")).toEqual({ name: "Nope Card", known: false });
  });
  it("rewrites item labels", () => {
    expect(printedItemLabel("Promissory Note: cf:generic")).toBe("Ceasefire");
    expect(printedItemLabel("2 trade goods")).toBe("2 trade goods");
  });
});

describe("sideLines", () => {
  it("prints names, not ids", () => {
    const side = { ...emptySide(), note: "cf:generic", actionCard: "sabo1", secret: "ans" };
    expect(sideLines(side)).toEqual([
      "Promissory Note: Ceasefire",
      "Action Card: Sabotage",
      "Secret Objective: Adapt New Strategies",
    ]);
    expect(sideLines({ ...emptySide(), note: "ra:jolnar" })).toEqual([
      "Promissory Note: Research Agreement (jolnar)",
    ]);
    expect(sideLines({ ...emptySide(), note: "mystery_pact:generic" })).toEqual([
      "Promissory Note: Mystery Pact",
    ]);
  });
});

describe("Quick deal labels", () => {
  const opt = (id: string, label: string, payload: Record<string, unknown> = {}): ChoiceOptionDto => ({
    id,
    kind: "offer",
    label,
    payload,
  });

  it("renders note ids as printed names and keeps the engine label for debugging", () => {
    const sell = decodeTradeOption(opt("pncf:generic:2", "sell cf:generic for 2 trade goods"));
    expect(sell.label).toBe("sell Ceasefire for 2 trade goods");
    expect(sell.engineLabel).toBe("sell cf:generic for 2 trade goods");
    expect(decodeTradeOption(opt("nncf:generic>ra:jolnar", "give the note cf:generic for the note ra:jolnar")).label).toBe(
      "give the note Ceasefire for the note Research Agreement (jolnar)",
    );
    expect(decodeTradeOption(opt("npra:jolnar:4", "pay 4 trade goods for the note ra:jolnar")).label).toBe(
      "pay 4 trade goods for the note Research Agreement (jolnar)",
    );
    expect(decodeTradeOption(opt("acsabo1:1", "sell the action card sabo1 for 1 trade good")).label).toBe(
      "sell the action card Sabotage for 1 trade good",
    );
    expect(decodeTradeOption(opt("pnmystery_pact:generic:0", "give mystery_pact:generic")).label).toBe(
      "give Mystery Pact",
    );
  });

  it("keeps the staged data on the raw ids", () => {
    const deal = decodeTradeOption(opt("pncf:generic:2", "sell cf:generic for 2 trade goods"));
    expect(dealToStaged(deal).give.note).toBe("cf:generic");
  });

  it("leaves an option it cannot decode alone", () => {
    const odd = decodeTradeOption(opt("zz", "do something with cf:generic"));
    expect(odd.label).toBe("do something with cf:generic");
  });
});

describe("humanizeOfferPrompt", () => {
  it("replaces ids in an answer prompt", () => {
    expect(
      humanizeOfferPrompt(
        "Hacan gives 2 trade goods, the action card sabo1 for 3 commodities, ra:jolnar -- accept?",
      ),
    ).toBe(
      "Hacan gives 2 trade goods, the action card Sabotage for 3 commodities, Research Agreement (jolnar) -- accept?",
    );
    expect(humanizeOfferPrompt("Sol gives cf:generic for support_for_throne -- accept?")).toBe(
      "Sol gives Ceasefire for Support for the Throne -- accept?",
    );
    expect(humanizeOfferPrompt("Sol gives mystery_pact:generic for nothing -- accept?")).toBe(
      "Sol gives Mystery Pact for nothing -- accept?",
    );
  });
  it("passes other prompts through", () => {
    expect(humanizeOfferPrompt("something else entirely")).toBe("something else entirely");
  });
});
