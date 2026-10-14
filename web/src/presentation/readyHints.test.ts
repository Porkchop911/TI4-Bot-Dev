import { describe, expect, it } from "vitest";
import { readReadyHints, spenderAnswer } from "./readyHints.ts";

describe("readReadyHints", () => {
  it("parses planets and spender status", () => {
    const hints = readReadyHints({
      ready_hints: {
        planets: [{ id: "lodor", name: "Lodor", resources: 3, influence: 1 }],
        spenders: [{ card: "Leadership", spends: "influence", status: "played" }],
      },
    });
    expect(hints?.planets[0]).toEqual({ id: "lodor", name: "Lodor", resources: 3, influence: 1 });
    expect(hints?.spenders[0].status).toBe("played");
  });
  it("is null when absent or malformed", () => {
    expect(readReadyHints(undefined)).toBeNull();
    expect(readReadyHints({ ready_hints: { planets: [], spenders: [{ card: "x", status: "?" }] } })).toBeNull();
  });
  it("answers yes only for played", () => {
    expect(spenderAnswer("played").played).toBe(true);
    expect(spenderAnswer("to_come").text).toContain("still to come");
  });
});
