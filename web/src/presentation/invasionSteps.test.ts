import { describe, expect, it } from "vitest";
import type { InvasionStepView, PlacedUnitView } from "../protocol/types.ts";
import { formatCounts, shooterLabel, stepOutcomeText, summarizeInvasionStep } from "./invasionSteps.ts";

const INVADER = "player_inv";
const RIVAL = "player_rival";
const unit = (owner: string, unit_type: string, damaged = false): PlacedUnitView => ({ owner, unit_type, damaged, planet: "jord", galvanized: false });
const die = (player: string, group: string, face: number, target: number) => ({ planet: "jord", player, group, face, target, hit: face >= target });

const bombardment: InvasionStepView = {
  planet: "jord",
  kind: "bombardment",
  round: 0,
  before: [unit(RIVAL, "infantry"), unit(RIVAL, "infantry"), unit(RIVAL, "mech"), unit(RIVAL, "pds")],
  after: [unit(RIVAL, "infantry"), unit(RIVAL, "mech", true), unit(RIVAL, "pds")],
  dice: [die(INVADER, "dreadnought", 9, 5), die(INVADER, "dreadnought", 7, 5), die(INVADER, "dreadnought", 1, 5)],
  hits: { [INVADER]: 2 },
  harrow_hits: 0,
};

describe("summarizeInvasionStep", () => {
  it("groups the dice by unit and threshold and counts hits", () => {
    const summary = summarizeInvasionStep(bombardment, INVADER)!;
    expect(summary.dice).toBe(3);
    expect(summary.hits).toBe(2);
    expect(summary.shooters).toHaveLength(1);
    expect(shooterLabel("bombardment", summary.shooters[0])).toBe("Dreadnought, Bombardment 5 x3");
  });

  it("reports what the hits cost, leaving structures out", () => {
    const summary = summarizeInvasionStep(bombardment, INVADER)!;
    expect(summary.targets).toHaveLength(1);
    expect(formatCounts(summary.targets[0].before)).toBe("2 Infantry, 1 Mech");
    expect(formatCounts(summary.targets[0].lost)).toBe("1 Infantry");
    expect(formatCounts(summary.targets[0].sustained)).toBe("1 Mech");
    expect(summary.destroyed).toBe(1);
  });

  it("is null for ground combat rounds", () => {
    expect(summarizeInvasionStep({ ...bombardment, kind: "ground_round" }, INVADER)).toBeNull();
    expect(summarizeInvasionStep(null, INVADER)).toBeNull();
  });

  it("aims Space Cannon Defense at the invader's landed forces", () => {
    const step: InvasionStepView = {
      planet: "jord",
      kind: "space_cannon_defense",
      round: 0,
      before: [unit(RIVAL, "pds"), unit(INVADER, "infantry"), unit(INVADER, "infantry")],
      after: [unit(RIVAL, "pds"), unit(INVADER, "infantry")],
      dice: [die(RIVAL, "pds", 8, 6), die(RIVAL, "pds", 2, 6)],
      hits: { [RIVAL]: 1 },
      harrow_hits: 0,
    };
    const summary = summarizeInvasionStep(step, INVADER)!;
    expect(summary.shooters[0].seat).toBe(RIVAL);
    expect(summary.targets.map((t) => t.seat)).toEqual([INVADER]);
    expect(stepOutcomeText(summary, INVADER)).toBe(
      `Space Cannon Defense on Jord: 1 hit from 2 dice. ${INVADER}: destroyed 1 Infantry.`,
    );
  });

  it("says plainly when nothing was hit", () => {
    const step = { ...bombardment, dice: [die(INVADER, "dreadnought", 1, 5)], after: bombardment.before, hits: {} };
    expect(stepOutcomeText(summarizeInvasionStep(step, INVADER)!, INVADER)).toMatch(/no hits/);
  });
});
