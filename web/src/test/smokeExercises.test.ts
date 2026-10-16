import { describe, expect, it } from "vitest";
import { shotCapFromEnv } from "../../e2e/smokeExercises";
import { prepConfigFromEnv } from "../../e2e/smokePrep";

describe("smoke exercise knobs", () => {
  it("caps screenshots at 20 by default and accepts a number or 'all'", () => {
    expect(shotCapFromEnv(undefined)).toBe(20);
    expect(shotCapFromEnv("all")).toBe(20);
    expect(shotCapFromEnv("4")).toBe(4);
    expect(shotCapFromEnv("0")).toBe(0);
    expect(shotCapFromEnv("nonsense")).toBe(20);
  });

  it("plans half of the secondaries, 90% of them in Auto, unless switched off", () => {
    expect(prepConfigFromEnv({})).toEqual({ enabled: true, probability: 0.5, autoProbability: 0.9 });
    expect(prepConfigFromEnv({ TI4_SMOKE_PREP: "0" }).enabled).toBe(false);
    expect(prepConfigFromEnv({ TI4_SMOKE_PREP_PROBABILITY: "1", TI4_SMOKE_PREP_AUTO_PROBABILITY: "0" })).toEqual({
      enabled: true,
      probability: 1,
      autoProbability: 0,
    });
    expect(prepConfigFromEnv({ TI4_SMOKE_PREP_PROBABILITY: "7" }).probability).toBe(1);
  });
});
