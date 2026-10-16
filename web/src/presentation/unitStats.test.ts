import { describe, it, expect } from "vitest";
import { resolveUnit } from "./factionInfo.ts";
import {
  describeUnitStats,
  diceStat,
  formatDice,
  formatUnitCost,
  hitChancePercent,
  keywordChips,
} from "./unitStats.ts";

const stats = (key: string, faction?: string, techs: string[] = []) => {
  const resolved = resolveUnit(key, faction, techs);
  if (!resolved) throw new Error(`no unit ${key}`);
  return describeUnitStats(resolved);
};

describe("hit chance", () => {
  it("is (11 - N) / 10 per die", () => {
    expect(hitChancePercent(9)).toBe(20);
    expect(hitChancePercent(8)).toBe(30);
    expect(hitChancePercent(5)).toBe(60);
    expect(hitChancePercent(3)).toBe(80);
    expect(hitChancePercent(1)).toBe(100);
  });
  it("scales expected hits by the dice and formats per die", () => {
    expect(diceStat(3, 3)).toMatchObject({ dice: 3, chancePercent: 80, expectedHits: 2.4 });
    expect(formatDice(diceStat(8, 2)!)).toBe("8 ×2 · 30% per die");
    expect(formatDice(diceStat(9, 1)!)).toBe("9 · 20%");
  });
  it("has no dice stat without a value or with zero dice", () => {
    expect(diceStat(undefined, 1)).toBeUndefined();
    expect(diceStat(8, 0)).toBeUndefined();
  });
});

describe("generic units match the printed cards", () => {
  it.each([
    ["carrier", 3, 9, 1, 1, 4],
    ["destroyer", 1, 9, 1, 2, undefined],
    ["cruiser", 2, 7, 1, 2, undefined],
    ["dreadnought", 4, 5, 1, 1, 1],
    ["warsun", 12, 3, 3, 2, 6],
  ])("%s", (key, cost, hitsOn, dice, move, capacity) => {
    const s = stats(key);
    expect(s.cost).toBe(cost);
    expect(s.combat).toMatchObject({ hitsOn, dice });
    expect(s.move).toBe(move);
    expect(s.capacity).toBe(capacity);
  });
  it("fighter and infantry cost 2 for 1 and have no movement", () => {
    expect(formatUnitCost(stats("fighter").cost!)).toBe("2 for 1");
    expect(formatUnitCost(stats("infantry").cost!)).toBe("2 for 1");
    expect(stats("infantry").combat).toMatchObject({ hitsOn: 8, chancePercent: 30 });
    expect(stats("fighter").move).toBeUndefined();
  });
  it("keywords: sustain, bombardment, anti-fighter barrage", () => {
    expect(keywordChips(stats("dreadnought")).map((c) => c.text)).toEqual([
      "Sustain",
      "Bombard 5 · 60%",
    ]);
    expect(keywordChips(stats("destroyer")).map((c) => c.text)).toEqual(["AFB 9 ×2 · 20% per die"]);
    expect(stats("warsun").disablesPlanetaryShield).toBe(true);
    expect(keywordChips(stats("cruiser"))).toEqual([]);
  });
});

describe("upgrades and faction variants", () => {
  it("the owned II card replaces the base card", () => {
    expect(stats("dreadnought").move).toBe(1);
    const two = stats("dreadnought", undefined, ["dn2"]);
    expect(two.move).toBe(2);
    expect(two.upgraded).toBe(true);
    expect(stats("fighter", undefined, ["ff2"])).toMatchObject({ move: 2, combat: { hitsOn: 8 } });
  });
  it("Sol's Advanced Carrier II is bigger, faster and sustains", () => {
    const s = stats("carrier", "sol", ["ac2"]);
    expect(s).toMatchObject({ move: 2, capacity: 8, sustain: true, factionSpecific: true, upgraded: true });
  });
  it("faction flagships and mechs use their own numbers", () => {
    expect(stats("flagship", "sol")).toMatchObject({ capacity: 12, combat: { hitsOn: 5, dice: 2 } });
    expect(stats("flagship", "letnev").bombardment).toMatchObject({ hitsOn: 5, dice: 3 });
    const xxcha = stats("flagship", "xxcha");
    expect(xxcha.spaceCannon).toMatchObject({ hitsOn: 5, dice: 3, deep: true });
    expect(keywordChips(xxcha).map((c) => c.text)).toContain("Space cannon 5 ×3 · 60% per die");
    expect(stats("mech", "l1z1x").bombardment).toMatchObject({ hitsOn: 8 });
    expect(stats("mech", "hacan")).toMatchObject({ cost: 2, combat: { hitsOn: 6, chancePercent: 50 } });
    expect(stats("infantry", "sol").combat?.hitsOn).toBe(7); // Spec Ops
  });
});

describe("units without combat", () => {
  it("a PDS and a space dock have no combat value", () => {
    expect(stats("pds").combat).toBeUndefined();
    expect(stats("pds").spaceCannon).toMatchObject({ hitsOn: 6, chancePercent: 50 });
    expect(stats("pds").planetaryShield).toBe(true);
    const dock = stats("spacedock");
    expect(dock.combat).toBeUndefined();
    expect(dock.production).toBe("+2");
    expect(dock.capacity).toBe(3);
  });
});
