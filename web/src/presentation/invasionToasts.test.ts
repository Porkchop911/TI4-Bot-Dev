import { describe, expect, it } from "vitest";
import type { BoardView, InvasionStepView, PlacedUnitView } from "../protocol/types.ts";
import { trackInvasion, type InvasionTracker } from "./invasionToasts.ts";

const INVADER = "player_inv";
const RIVAL = "player_rival";
const g = (owner: string, unit_type: string, damaged = false): PlacedUnitView => ({ owner, unit_type, damaged, planet: "jord", galvanized: false });
const many = (n: number, type: string, owner: string) => Array.from({ length: n }, () => g(owner === "" ? RIVAL : owner, type));
const die = (player: string, group: string, face: number, target: number) => ({ planet: "jord", player, group, face, target, hit: face >= target });

function board(units: PlacedUnitView[], step: InvasionStepView | null, opts: { over?: boolean; controller?: string | null } = {}): BoardView {
  return {
    systems: {
      "18": {
        system_id: "18",
        command_tokens: [],
        planets: { jord: { planet_id: "jord", controlled_by: opts.controller === undefined ? RIVAL : opts.controller, exhausted: false } },
        units,
      },
    },
    invasion: opts.over
      ? null
      : {
          system_id: "18",
          invasion_seq: 3,
          invader: INVADER,
          phase: "x",
          planets: ["jord"],
          current_planet: "jord",
          defender: RIVAL,
          ground_round: 0,
          last_step: step,
        },
  } as unknown as BoardView;
}

const defenders = [...many(2, "infantry", RIVAL), g(RIVAL, "pds")];
const bombardment: InvasionStepView = {
  planet: "jord",
  kind: "bombardment",
  round: 0,
  before: defenders,
  after: [g(RIVAL, "infantry"), g(RIVAL, "pds")],
  dice: [die(INVADER, "dreadnought", 9, 5), die(INVADER, "dreadnought", 1, 5)],
  hits: { [INVADER]: 1 },
  harrow_hits: 0,
};
const landed = [g(INVADER, "infantry"), g(INVADER, "infantry")];
const cannon: InvasionStepView = {
  planet: "jord",
  kind: "space_cannon_defense",
  round: 0,
  before: [...bombardment.after, ...landed],
  after: [...bombardment.after, g(INVADER, "infantry")],
  dice: [die(RIVAL, "pds", 7, 6)],
  hits: { [RIVAL]: 1 },
  harrow_hits: 0,
};
const groundDone: InvasionStepView = {
  planet: "jord",
  kind: "ground_round",
  round: 1,
  before: [...cannon.after],
  after: [g(RIVAL, "pds"), g(INVADER, "infantry")],
  dice: [],
  hits: { [INVADER]: 1, [RIVAL]: 0 },
  harrow_hits: 0,
};

describe("trackInvasion", () => {
  it("toasts the bombardment once when its step first shows", () => {
    let t: InvasionTracker | null = null;
    const first = trackInvasion(t, board(defenders, null));
    t = first.tracker;
    expect(first.toasts).toEqual([]);
    const second = trackInvasion(t, board(bombardment.after, bombardment));
    expect(second.toasts).toHaveLength(1);
    expect(second.toasts[0].kind).toBe("bombardment");
    expect(second.toasts[0].text).toContain("1 hit from 2 dice");
    expect(second.toasts[0].text).toContain("destroyed 1 Infantry");
    const again = trackInvasion(second.tracker, board(bombardment.after, bombardment));
    expect(again.toasts).toEqual([]);
  });

  it("stays quiet about steps already shown when first seen", () => {
    const result = trackInvasion(null, board(bombardment.after, bombardment), true);
    expect(result.toasts).toEqual([]);
    const next = trackInvasion(result.tracker, board(cannon.after, cannon));
    expect(next.toasts.map((t) => t.kind)).toEqual(["space_cannon_defense"]);
    // A partial invasion has no summary either.
    const end = trackInvasion(next.tracker, board([], null, { over: true }));
    expect(end.toasts).toEqual([]);
  });

  it("summarises the whole invasion when it ends", () => {
    let t = trackInvasion(null, board(defenders, null)).tracker;
    t = trackInvasion(t, board(bombardment.after, bombardment)).tracker;
    t = trackInvasion(t, board(cannon.after, cannon)).tracker;
    t = trackInvasion(t, board(groundDone.after, groundDone)).tracker;
    const end = trackInvasion(t, board(groundDone.after, groundDone, { over: true, controller: INVADER }));
    expect(end.tracker).toBeNull();
    expect(end.toasts).toHaveLength(1);
    const text = end.toasts[0].text;
    expect(end.toasts[0].kind).toBe("summary");
    expect(text).toContain(`Invasion of Jord by ${INVADER}`);
    expect(text).toContain("landed 2 Infantry");
    expect(text).toContain("lost 1 Infantry");
    expect(text).toContain(`${RIVAL} lost 2 Infantry`);
    expect(text).toContain("bombardment 1 hit");
    expect(text).toContain("Space Cannon 1 hit");
    expect(text).toContain("ground combat won after 1 round");
    expect(text).toContain(`${INVADER} now controls Jord`);
  });

  it("says when nobody fought", () => {
    let t = trackInvasion(null, board(landed, null, { controller: null })).tracker;
    const end = trackInvasion(t, board(landed, null, { over: true, controller: INVADER }));
    expect(end.toasts[0].text).toContain("no ground combat");
    expect(end.toasts[0].text).toContain(`${INVADER} now controls Jord`);
    t = null;
  });
});
