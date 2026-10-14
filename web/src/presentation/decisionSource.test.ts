import { describe, expect, it } from "vitest";
import { describeDecisionHeader, formatDecisionSource } from "./decisionSource.ts";

describe("describeDecisionHeader", () => {
  it("names the topic and keeps a rule source in the strip, not the headline", () => {
    const info = describeDecisionHeader({
      context: {
        subtype: "gain_command_token",
        source: { Rule: "52.4" },
        phase: "Status",
        round: 3,
      },
    });
    expect(info.eyebrow).toBe("Command tokens");
    expect(info.chips).toEqual(["Status phase · Round 3", "Rule 52.4"]);
  });

  it("leads with the card or ability that is asking", () => {
    const info = describeDecisionHeader({
      context: {
        subtype: "politics_place_agenda",
        source: { StrategyCard: { card: "pok3politics", secondary: true } },
        phase: "Action",
        round: 2,
        optional: true,
      },
    });
    expect(info.eyebrow).toBe("Politics (secondary)");
    expect(info.chips).toEqual(["Action phase · Round 2", "Optional"]);
  });

  it("has nothing to show without a context", () => {
    expect(describeDecisionHeader({})).toEqual({ eyebrow: null, chips: [] });
    expect(formatDecisionSource(undefined)).toBeNull();
  });
});
