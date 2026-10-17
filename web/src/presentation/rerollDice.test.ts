import { describe, expect, it } from "vitest";
import { describeRerollDice, rerollIntents, rerollOutcome } from "./rerollDice.ts";

const dice = [
  { unit: "infantry", unit_index: 0, die: 0, face: 3, bonus: 0, hits_on: 8, rerolled: false },
  { unit: "infantry", unit_index: 0, die: 1, face: 9, bonus: 0, hits_on: 8, rerolled: false },
  { unit: "mech", unit_index: 1, die: 0, face: 5, bonus: 1, hits_on: 6, rerolled: true },
];
const choice = (current: number) => ({
  context: { subtype: "fire_team_reroll", source: { ActionCard: "fire_team" } } as never,
  options: [
    { id: `reroll|0:${current}`, kind: "reroll_die", label: "reroll", payload: {} },
    { id: "decline", kind: "decline", label: "Decline", payload: {} },
  ],
  details: { dice, current },
});

describe("reroll dice (Fire Team and the other roll cards)", () => {
  it("lists every die with its face, threshold, hit and odds, grouped by unit", () => {
    const view = describeRerollDice(choice(0));
    expect(view?.source).toBe("Fire Team");
    expect(view?.dice.map((die) => [die.face, die.hit])).toEqual([
      [3, false],
      [9, true],
      [5, true], // 5 + 1 reaches the 6
    ]);
    expect(view?.groups.map((group) => [group.unit, group.dice.length, group.hitsOn])).toEqual([
      ["infantry", 2, 8],
      ["mech", 1, 6],
    ]);
    expect(view?.dice[0].chance).toBeCloseTo(0.3);
    expect(view?.dice[2].rerolled).toBe(true);
    expect(view?.dice[0].optionId).toBe("reroll|0:0");
    expect(view?.dice[2].optionId).toBe("reroll|1:0");
  });

  it("marks dice whose question was answered already", () => {
    const view = describeRerollDice(choice(2));
    expect(view?.dice.map((die) => die.decided)).toEqual([true, true, false]);
    expect(rerollIntents(view!, new Set([2]))).toHaveLength(1);
  });

  it("is nothing for any other decision, or without the dice", () => {
    expect(describeRerollDice({ ...choice(0), options: [{ id: "decline", kind: "decline", label: "x", payload: {} }] })).toBeNull();
    expect(describeRerollDice({ ...choice(0), details: undefined })).toBeNull();
  });

  it("counts the hits now and the hits to expect from the ticked dice", () => {
    const view = describeRerollDice(choice(0))!;
    expect(rerollOutcome(view, new Set())).toEqual({ now: 2, expected: 2, best: 2, worst: 2 });
    // Redrawing the miss: 3 is replaced by a 30% shot; the hits kept stay.
    expect(rerollOutcome(view, new Set([0]))).toEqual({ now: 2, expected: 2.3, best: 3, worst: 2 });
    // Redrawing a hit can lose it.
    expect(rerollOutcome(view, new Set([1]))).toMatchObject({ expected: 1.3, best: 2, worst: 1 });
  });

  it("answers each remaining question: the die's own option if ticked, else decline", () => {
    const view = describeRerollDice(choice(0))!;
    const intents = rerollIntents(view, new Set([1]));
    const asked = (index: number) => [
      { id: view.dice[index].optionId, kind: "reroll_die", label: "", payload: {} },
      { id: "decline", kind: "decline", label: "", payload: {} },
    ];
    expect(intents).toHaveLength(3);
    expect(asked(0).filter(intents[0]).map((o) => o.id)).toEqual(["decline"]);
    expect(asked(1).filter(intents[1]).map((o) => o.id)).toEqual(["reroll|0:1"]);
    expect(asked(2).filter(intents[2]).map((o) => o.id)).toEqual(["decline"]);
  });
});
