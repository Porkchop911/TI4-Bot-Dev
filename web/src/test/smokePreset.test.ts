import { describe, expect, it } from "vitest";
import {
  basePreset,
  cardSetFromEnv,
  createGameBody,
  mapTemplateFromEnv,
  missingExpected,
  parseExpect,
  presetFromEnv,
} from "../../e2e/smokePreset";

describe("smoke start preset", () => {
  it("treats an unset or empty TI4_SMOKE_PRESET as a normal opening", () => {
    expect(presetFromEnv(undefined)).toBeUndefined();
    expect(presetFromEnv("")).toBeUndefined();
    expect(presetFromEnv("  ")).toBeUndefined();
  });

  it("accepts the combat preset and rejects an unknown one", () => {
    expect(presetFromEnv("combat")).toBe("combat");
    expect(() => presetFromEnv("nope")).toThrow(/unknown TI4_SMOKE_PRESET/);
  });

  it("accepts a +rot suffix on a known preset and uses the base preset's expectations", () => {
    expect(presetFromEnv("invasion+rot")).toBe("invasion+rot");
    expect(() => presetFromEnv("nope+rot")).toThrow(/unknown TI4_SMOKE_PRESET/);
    expect(parseExpect("preset", "invasion+rot")).toEqual(parseExpect("preset", "invasion"));
    expect(parseExpect("preset", "invasion").length).toBeGreaterThan(0);
  });

  it("accepts the roster and short suffixes in any order, with named factions", () => {
    for (const ok of ["world", "explore+short", "notes+rot", "combat+fac", "techs+short+fac:naalu:mentak", "leaders+fac:yin+short", "combat+agenda", "world+agenda+short+fac:naalu"]) {
      expect(presetFromEnv(ok)).toBe(ok);
    }
    for (const bad of ["world+nope", "combat+fac:Naalu", "combat+short+", "combat+agenda:x", "agenda+", "nope+fac", "+rot"]) {
      expect(() => presetFromEnv(bad)).toThrow(/unknown TI4_SMOKE_PRESET/);
    }
    expect(basePreset("techs+short+fac")).toBe("techs");
    expect(basePreset(undefined)).toBe("");
    expect(parseExpect("preset", "invasion+fac+short")).toEqual(parseExpect("preset", "invasion"));
  });

  it("sends map_template only when one was asked for", () => {
    expect(mapTemplateFromEnv(undefined)).toBeUndefined();
    expect(mapTemplateFromEnv(" ")).toBeUndefined();
    expect(mapTemplateFromEnv(" 3pInPersonHyperlanes ")).toBe("3pInPersonHyperlanes");
    expect(createGameBody(3, 7, "explore", undefined, "3pInPersonHyperlanes")).toEqual({
      player_count: 3,
      seed: 7,
      nickname: "E2E Host",
      start_preset: "explore",
      map_template: "3pInPersonHyperlanes",
    });
    expect(createGameBody(3, 7)).not.toHaveProperty("map_template");
  });

  it("sends start_preset only when one was asked for", () => {
    expect(createGameBody(3, 7)).toEqual({ player_count: 3, seed: 7, nickname: "E2E Host" });
    expect(createGameBody(3, 7, "combat")).toEqual({
      player_count: 3,
      seed: 7,
      nickname: "E2E Host",
      start_preset: "combat",
    });
  });

  it("sends strategy_card_set only when a set was asked for", () => {
    expect(createGameBody(3, 7, undefined, "pok")).toEqual({
      player_count: 3,
      seed: 7,
      nickname: "E2E Host",
      strategy_card_set: "pok",
    });
    expect(cardSetFromEnv(undefined)).toBeUndefined();
    expect(cardSetFromEnv(" te ")).toBe("te");
    expect(() => cardSetFromEnv("nope")).toThrow(/unknown TI4_SMOKE_CARD_SET/);
  });

  it("parses expectations: plain, any-of and at-least-N", () => {
    expect(parseExpect(undefined)).toEqual([]);
    expect(parseExpect("a, b|c, d|e|f>=2")).toEqual([
      { subtypes: ["a"], atLeast: 1 },
      { subtypes: ["b", "c"], atLeast: 1 },
      { subtypes: ["d", "e", "f"], atLeast: 2 },
    ]);
  });

  it("reports only the expectations the run missed", () => {
    const seen = { a: 3, c: 1, d: 1 };
    const wanted = parseExpect("a,b|c,d|e|f>=2,x");
    expect(missingExpected(seen, wanted)).toEqual(["d|e|f (need 2, saw 1)", "x (need 1, saw 0)"]);
  });
});
