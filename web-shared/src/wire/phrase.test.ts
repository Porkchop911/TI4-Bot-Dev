import { describe, expect, it } from "vitest";
import type { Phrase } from "./index.ts";
import { fillPhrase, sayPhrase, type SubjectNames } from "./phrase.ts";

const names: SubjectNames = {
  seat: ({ player }) => player.toUpperCase(),
  seat_possessive: ({ player }) => `${player.toUpperCase()}'s`,
  system: ({ system }) => `#${system}`,
  planet: ({ planet }) => planet,
  unit: ({ unit_type }) => unit_type,
  units: ({ unit_type, count }) => `${count}×${unit_type}`,
  agenda: ({ card }) => card,
  strategy_card: ({ card }) => card,
  note: ({ card, owner }) => `${card}/${owner}`,
  action_card: ({ card }) => card,
  secret_objective: ({ card }) => card,
};

describe("a phrase of the engine", () => {
  const phrase: Phrase = {
    text: "Destroys {0} {1} in {2}.",
    refs: {
      0: { kind: "seat_possessive", player: "b" },
      1: { kind: "unit", unit_type: "dreadnought" },
      2: { kind: "system", system: "27" },
    },
  };

  it("says each subject by its kind", () => {
    expect(sayPhrase(phrase, names)).toBe("Destroys B's dreadnought in #27.");
  });

  it("keeps a key without a subject and a text without keys", () => {
    expect(
      fillPhrase({ text: "{0} and {x}", refs: { 0: { kind: "seat", player: "a" } } }, () => "A"),
    ).toBe("A and {x}");
    expect(sayPhrase({ text: "Cannot retreat", refs: {} }, names)).toBe("Cannot retreat");
  });
});
