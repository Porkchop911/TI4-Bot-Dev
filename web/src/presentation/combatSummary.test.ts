import { describe, expect, it } from "vitest";
import type { CombatView, PlacedUnitView } from "../protocol/types.ts";
import { summarizeCombat } from "./combatSummary.ts";

const u = (unit_type: string, owner: string, damaged = false): PlacedUnitView => ({
  unit_type,
  owner,
  damaged,
});
const combat = (start: PlacedUnitView[], extra: Partial<CombatView> = {}): CombatView => ({
  system_id: "18",
  round: 2,
  phase: "complete",
  attacker: "a",
  defender: "d",
  round_start: start,
  attacker_hits: 3,
  defender_hits: 1,
  ...extra,
});

describe("summarizeCombat", () => {
  const start = [
    u("fighter", "a"),
    u("fighter", "a"),
    u("dreadnought", "a"),
    u("cruiser", "d"),
    u("destroyer", "d"),
  ];

  it("reports losses, sustained damage and the winner of a one-sided fight", () => {
    const now = [u("fighter", "a"), u("dreadnought", "a", true)];
    const summary = summarizeCombat(combat(start), now)!;
    expect(summary.verdict).toEqual({ kind: "won", winner: "a", loser: "d" });
    expect(summary.attacker.lost).toEqual([{ unit: "fighter", count: 1 }]);
    expect(summary.attacker.sustained).toEqual([{ unit: "dreadnought", count: 1 }]);
    expect(summary.attacker.remaining).toBe(2);
    expect(summary.attacker.hits).toBe(3);
    expect(summary.defender.lost).toEqual([
      { unit: "cruiser", count: 1 },
      { unit: "destroyer", count: 1 },
    ]);
    expect(summary.defender.remaining).toBe(0);
  });

  it("does not count damage that was already there at the start of the round", () => {
    const damagedStart = [u("dreadnought", "a", true), u("cruiser", "d")];
    const now = [u("dreadnought", "a", true)];
    const summary = summarizeCombat(combat(damagedStart), now)!;
    expect(summary.attacker.sustained).toEqual([]);
  });

  it("calls a fight with ships on both sides unresolved, and a wipe-out mutual", () => {
    expect(
      summarizeCombat(combat(start), [u("fighter", "a"), u("cruiser", "d")])!.verdict,
    ).toEqual({ kind: "both_remain" });
    expect(summarizeCombat(combat(start), [])!.verdict).toEqual({ kind: "annihilated" });
  });

  it("ignores ground forces and returns null without the starting fleets", () => {
    const withGround = [...start, { ...u("infantry", "a"), planet: "jord" }];
    const now = [u("fighter", "a"), u("fighter", "a"), u("dreadnought", "a"), { ...u("infantry", "a"), planet: "jord" }];
    expect(summarizeCombat(combat(withGround), now)!.attacker.lost).toEqual([]);
    expect(summarizeCombat(combat(start, { round_start: undefined }), [])).toBeNull();
  });
});
