import { describe, expect, it } from "vitest";
import type { InvasionStepView, PlacedUnitView } from "../protocol/types.ts";
import { summarizeGroundCombat } from "./groundCombatSummary.ts";

const u = (unit_type: string, owner: string, damaged = false): PlacedUnitView => ({
  unit_type,
  owner,
  planet: "jord",
  damaged,
});
const step = (
  before: PlacedUnitView[],
  after: PlacedUnitView[],
  extra: Partial<InvasionStepView> = {},
): InvasionStepView => ({
  planet: "jord",
  kind: "ground_round",
  round: 2,
  before,
  after,
  dice: [],
  hits: { a: 3, d: 1 },
  harrow_hits: 0,
  ...extra,
});
const start = [u("infantry", "a"), u("infantry", "a"), u("mech", "a"), u("infantry", "d"), u("infantry", "d")];

describe("summarizeGroundCombat", () => {
  it("reports an attacker victory with units before and after", () => {
    const s = summarizeGroundCombat(step(start, [u("infantry", "a"), u("mech", "a", true)]), "a")!;
    expect(s.verdict).toEqual({ kind: "attacker_won", winner: "a", loser: "d" });
    expect(s.planet).toBe("jord");
    expect(s.attacker.before).toEqual([
      { unit: "infantry", count: 2 },
      { unit: "mech", count: 1 },
    ]);
    expect(s.attacker.after).toEqual([
      { unit: "infantry", count: 1 },
      { unit: "mech", count: 1 },
    ]);
    expect(s.attacker.lost).toEqual([{ unit: "infantry", count: 1 }]);
    expect(s.attacker.sustained).toEqual([{ unit: "mech", count: 1 }]);
    expect(s.attacker.hits).toBe(3);
    expect(s.defender.lost).toEqual([{ unit: "infantry", count: 2 }]);
    expect(s.defender.remaining).toBe(0);
    expect(s.defender.hits).toBe(1);
  });

  it("reports the defender holding when the invaders are wiped out", () => {
    const s = summarizeGroundCombat(step(start, [u("infantry", "d")]), "a")!;
    expect(s.verdict).toEqual({ kind: "defender_held", winner: "d", loser: "a" });
    expect(s.attacker.remaining).toBe(0);
    expect(s.defender.remaining).toBe(1);
  });

  it("reports mutual destruction", () => {
    const s = summarizeGroundCombat(step(start, []), "a")!;
    expect(s.verdict).toEqual({ kind: "mutual_destruction" });
  });

  it("returns null while both sides still have ground forces", () => {
    expect(summarizeGroundCombat(step(start, [u("infantry", "a"), u("infantry", "d")]), "a")).toBeNull();
  });

  it("ignores structures, other step kinds and missing steps", () => {
    const withPds = [...start, u("pds", "d")];
    const s = summarizeGroundCombat(step(withPds, [u("infantry", "a"), u("pds", "d")]), "a")!;
    expect(s.verdict.kind).toBe("attacker_won");
    expect(s.defender.before).toEqual([{ unit: "infantry", count: 2 }]);
    expect(summarizeGroundCombat(step(start, [], { kind: "bombardment" }), "a")).toBeNull();
    expect(summarizeGroundCombat(null, "a")).toBeNull();
  });

  it("does not count damage that was already there", () => {
    const s = summarizeGroundCombat(
      step([u("mech", "a", true), u("infantry", "d")], [u("mech", "a", true)]),
      "a",
    )!;
    expect(s.attacker.sustained).toEqual([]);
  });
});
