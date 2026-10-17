import { describe, expect, it } from "vitest";
import { describeLawOption, isLawPick } from "./lawPick.ts";

const repeal = {
  context: { subtype: "repeal_pick_repeal" } as never,
  details: {
    laws: {
      checks: { name: "Checks and Balances", type: "Law", text1: "For: x.", text2: "Against: y.", outcome: "against" },
      minister_exploration: {
        name: "Minister of Exploration",
        type: "Law",
        target: "Elect Player",
        text1: "After a player gains control of a planet, they may spend 1 trade good to explore it.",
        text2: "",
        outcome: "player_ab",
      },
    },
  },
};

describe("Repeal Law's law picker", () => {
  it("recognises the card's question", () => {
    expect(isLawPick(repeal)).toBe(true);
    expect(isLawPick({ context: { subtype: "spy_pick_player" } as never })).toBe(false);
  });

  it("names a law, says what it was elected onto and shows its printed text", () => {
    expect(describeLawOption(repeal, { id: "minister_exploration" })).toEqual({
      name: "Minister of Exploration",
      kind: "Law",
      elected: "player_ab",
      passed: null,
      text: ["After a player gains control of a planet, they may spend 1 trade good to explore it."],
    });
  });

  it("shows only the side of a For/Against law that passed", () => {
    expect(describeLawOption(repeal, { id: "checks" })).toMatchObject({
      passed: "Against",
      elected: null,
      text: ["Against: y."],
    });
  });

  it("falls back to a readable name when the engine sent no card text", () => {
    expect(describeLawOption({ context: repeal.context }, { id: "minister_commerce" })).toMatchObject({
      name: "Minister Commerce",
      text: [],
    });
    expect(describeLawOption({ context: { subtype: "other" } as never }, { id: "x" })).toBeNull();
  });
});
