import { describe, expect, it } from "vitest";
import type { BoardView, PlayerView } from "../protocol/types.ts";
import {
  formatDefenderUnits,
  planDefaultLanding,
  planetDefense,
  planetInvasionInfo,
  planetStanding,
  sameDraft,
  type PlanPlanet,
} from "./landingPlan.ts";

const p = (id: string, value: number, extra: Partial<PlanPlanet> = {}): PlanPlanet => ({
  id,
  value,
  extra: 0,
  defended: false,
  held: false,
  ...extra,
});
const all = () => true;
const counts = (plan: ReturnType<typeof planDefaultLanding>) => {
  const out: Record<string, string[]> = {};
  for (const l of plan.landings) (out[l.planet] ??= []).push(l.unit);
  return out;
};
const inf = (count: number) => ({ unit: "infantry", damaged: false, count });
const mech = (count: number) => ({ unit: "mech", damaged: false, count });

describe("planDefaultLanding", () => {
  it("puts everything on a single planet", () => {
    const plan = planDefaultLanding([p("a", 3)], [inf(4)], all);
    expect(counts(plan)).toEqual({ a: ["infantry", "infantry", "infantry", "infantry"] });
    expect(plan.split).toBe(false);
  });

  it("spreads over two uninhabited planets, extra units on the better one", () => {
    const plan = planDefaultLanding([p("lo", 1), p("hi", 4)], [inf(5)], all);
    expect(counts(plan)).toEqual({ hi: ["infantry", "infantry", "infantry"], lo: ["infantry", "infantry"] });
    expect(plan.split).toBe(true);
    expect(plan.uninhabited).toEqual(["hi", "lo"]);
  });

  it("six infantry over three uninhabited planets is 2/2/2", () => {
    const plan = planDefaultLanding([p("a", 1), p("b", 2), p("c", 3)], [inf(6)], all);
    expect(Object.values(counts(plan)).map((l) => l.length)).toEqual([2, 2, 2]);
  });

  it("with one defended planet the contested one keeps at least half, the rest go one each", () => {
    const planets = [p("def", 1, { defended: true }), p("u1", 3), p("u2", 2)];
    expect(counts(planDefaultLanding(planets, [inf(6)], all))).toMatchObject({
      def: ["infantry", "infantry", "infantry", "infantry"],
      u1: ["infantry"],
      u2: ["infantry"],
    });
    const three = counts(planDefaultLanding(planets, [inf(3)], all));
    expect(three.def).toHaveLength(2);
    expect(three.u1).toHaveLength(1);
    expect(three.u2).toBeUndefined();
    expect(counts(planDefaultLanding(planets, [inf(1)], all))).toEqual({ def: ["infantry"] });
  });

  it("with more planets than units only the most valuable planets get a force", () => {
    const plan = planDefaultLanding([p("a", 1), p("b", 5), p("c", 3)], [inf(2)], all);
    expect(counts(plan)).toEqual({ b: ["infantry"], c: ["infantry"] });
  });

  it("keeps infantry for the spread and sends the mech where it matters", () => {
    const planets = [p("def", 1, { defended: true }), p("u", 2)];
    expect(counts(planDefaultLanding(planets, [inf(2), mech(1)], all))).toEqual({
      def: ["mech", "infantry"],
      u: ["infantry"],
    });
    const free = counts(planDefaultLanding([p("lo", 1), p("hi", 4)], [inf(3), mech(1)], all));
    expect(free.hi![0]).toBe("mech");
    expect(free.lo![0]).toBe("infantry");
    expect(Object.values(free).flat()).toHaveLength(4);
  });

  it("odd counts give the extra unit to the best planet", () => {
    const plan = planDefaultLanding([p("a", 2), p("b", 2), p("c", 2)], [inf(7)], all);
    expect(plan.landings.filter((l) => l.planet === "a")).toHaveLength(3);
    expect(plan.landings.filter((l) => l.planet === "b")).toHaveLength(2);
    expect(plan.landings.filter((l) => l.planet === "c")).toHaveLength(2);
  });

  it("breaks value ties by extra (legendary, attachments) before the id", () => {
    const plan = planDefaultLanding([p("a", 2), p("z", 2, { extra: 10 })], [inf(3)], all);
    expect(plan.landings.filter((l) => l.planet === "z")).toHaveLength(2);
  });

  it("is deterministic regardless of input order", () => {
    const a = planDefaultLanding([p("a", 1), p("b", 2), p("c", 3)], [inf(4), mech(1)], all);
    const b = planDefaultLanding([p("c", 3), p("a", 1), p("b", 2)], [mech(1), inf(4)], all);
    expect(a).toEqual(b);
  });

  it("skips planets already held and uses them only as the last resort", () => {
    const plan = planDefaultLanding([p("held", 5, { held: true }), p("u", 1)], [inf(2)], all);
    expect(counts(plan)).toEqual({ u: ["infantry", "infantry"] });
    const only = planDefaultLanding([p("held", 5, { held: true })], [inf(2)], all);
    expect(counts(only)).toEqual({ held: ["infantry", "infantry"] });
  });

  it("never plans a landing that is not offered and never exceeds the stock", () => {
    const offered = (planet: string, unit: string) => !(planet === "b" && unit === "mech");
    const plan = planDefaultLanding([p("a", 1), p("b", 9)], [mech(1), inf(1)], offered);
    expect(plan.landings).toHaveLength(2);
    expect(plan.landings).not.toContainEqual({ planet: "b", unit: "mech", damaged: false });
    expect(planDefaultLanding([p("a", 1)], [], all).landings).toEqual([]);
    expect(planDefaultLanding([], [inf(3)], all).landings).toEqual([]);
  });

  it("treats damaged units as separate stock", () => {
    const plan = planDefaultLanding(
      [p("a", 2), p("b", 1)],
      [inf(1), { unit: "infantry", damaged: true, count: 1 }],
      all,
    );
    expect(plan.landings).toHaveLength(2);
    expect(plan.landings.find((l) => l.planet === "a")?.damaged).toBe(false);
  });
});

describe("sameDraft", () => {
  it("ignores order", () => {
    const a = { planet: "x", unit: "infantry", damaged: false };
    const b = { planet: "y", unit: "mech", damaged: false };
    expect(sameDraft([a, b], [b, a])).toBe(true);
    expect(sameDraft([a, a], [a, b])).toBe(false);
  });
});

const board = (extra: Record<string, unknown> = {}, tilePlanets: unknown[] = []) =>
  ({
    systems: {
      s: {
        system_id: "s",
        command_tokens: [],
        planets: {
          a: { planet_id: "a", exhausted: false },
          b: { planet_id: "b", exhausted: false, controlled_by: "p2" },
        },
        units: [],
        ...extra,
      },
    },
    map_tiles: [{ system_id: "s", label: "s", q: 0, r: 0, planets: tilePlanets }],
  }) as unknown as BoardView;

describe("planetStanding", () => {
  it("any rival unit, structures included, makes a planet defended", () => {
    const b = board({ units: [{ owner: "p2", unit_type: "pds", damaged: false, planet: "a" }] });
    expect(planetStanding(b, "s", "a", "me").standing).toBe("defended");
  });
  it("a planet controlled by someone else with no units is uninhabited", () => {
    expect(planetStanding(board(), "s", "b", "me").standing).toBe("uninhabited");
  });
  it("a planet of mine is held", () => {
    const b = board({ units: [{ owner: "me", unit_type: "infantry", damaged: false, planet: "a" }] });
    expect(planetStanding(b, "s", "a", "me").standing).toBe("held");
  });
});

describe("planetInvasionInfo", () => {
  const tile = (traits?: string[]) => [
    { id: "a", label: "A", resources: 1, influence: 1, ...(traits ? { traits } : {}) },
  ];
  const text = (b: BoardView, id: string, assigned: number) =>
    planetInvasionInfo(b, "s", id, "me", assigned).effects.map((e) => e.text);

  it("explore line per trait, only with units assigned", () => {
    for (const trait of ["cultural", "industrial", "hazardous"]) {
      const b = board({}, tile([trait]));
      expect(text(b, "a", 2)).toContain(`Explores on landing: ${trait}`);
      expect(text(b, "a", 0).join(" ")).toContain("No units assigned");
    }
  });

  it("dual trait is the invader's choice", () => {
    expect(text(board({}, tile(["cultural", "industrial"])), "a", 1).join(" ")).toContain(
      "cultural or industrial (your choice)",
    );
  });

  it("contested planets explore only if the ground combat is won", () => {
    const b = board(
      { units: [{ owner: "p2", unit_type: "infantry", damaged: false, planet: "a" }] },
      tile(["cultural"]),
    );
    const lines = text(b, "a", 3);
    expect(lines).toContain("If you win the ground combat: explore cultural");
    expect(lines[0]).toContain("Defended by p2");
  });

  it("no exploration for a planet already controlled or without a trait", () => {
    const b = board({}, [{ id: "b", label: "B", resources: 1, influence: 1, traits: ["cultural"] }]);
    expect(text(b, "b", 1).join(" ")).toContain("No exploration: planet is already controlled");
    expect(text(board({}, tile()), "a", 1)).toContain("No exploration (planet has no trait)");
  });

  it("legendary ability name and text come from the catalog", () => {
    const b = board({ planets: { avernus: { planet_id: "avernus", exhausted: false } } }, [
      { id: "avernus", label: "Avernus", resources: 2, influence: 0, legendary: true },
    ]);
    const effect = planetInvasionInfo(b, "s", "avernus", "me", 1).effects.find(
      (e) => e.kind === "legendary",
    );
    expect(effect?.text).toContain("The Nucleus");
    expect(effect?.detail).toContain("STAR FORGE");
  });

  it("lists attachments with their modifiers", () => {
    const b = board(
      { planets: { a: { planet_id: "a", exhausted: false, attachments: ["bioticstat"] } } },
      tile(["cultural"]),
    );
    expect(text(b, "a", 1).join(" ")).toContain(
      "Attached: Biotic Research Facility (+1 resources, +1 influence)",
    );
  });

  it("Mecatol notes the custodians token", () => {
    const b = board({ planets: { mr: { planet_id: "mr", exhausted: false } } }, [
      { id: "mr", label: "Mecatol Rex", resources: 1, influence: 6 },
    ]);
    expect(text(b, "mr", 1).join(" ")).toContain("Custodians token already removed");
  });

  it("a held planet shows no capture effects", () => {
    const b = board(
      { units: [{ owner: "me", unit_type: "infantry", damaged: false, planet: "a" }] },
      tile(["cultural"]),
    );
    expect(text(b, "a", 0).join(" ")).not.toContain("xplore");
  });
});

describe("planetDefense", () => {
  const unit = (owner: string, unit_type: string, planet?: string, damaged = false) => ({
    owner,
    unit_type,
    planet,
    damaged,
  });
  const board = (units: ReturnType<typeof unit>[]) =>
    ({ systems: { "18": { system_id: "18", command_tokens: [], planets: {}, units } } }) as unknown as BoardView;
  const seat = (id: string, faction: string, technologies: string[] = []) =>
    ({ id, faction, technologies }) as unknown as PlayerView;

  it("counts another player's ground forces by type, PDS and structures on that planet only", () => {
    const defense = planetDefense(
      board([
        unit("me", "infantry"),
        unit("me", "infantry", "jord"),
        unit("rival", "infantry", "jord"),
        unit("rival", "infantry", "jord"),
        unit("rival", "infantry", "jord"),
        unit("rival", "mech", "jord", true),
        unit("rival", "pds", "jord"),
        unit("rival", "space_dock", "jord"),
        unit("rival", "infantry", "moll"),
        unit("rival", "carrier"),
      ]),
      "18",
      "jord",
      "me",
      { rival: seat("rival", "hacan") },
    );
    expect(defense.forces).toHaveLength(1);
    const [force] = defense.forces;
    expect(force!.owner).toBe("rival");
    expect(force!.groundTotal).toBe(4);
    expect(formatDefenderUnits(force!.ground)).toBe("1 Mech (1 damaged), 3 Infantry");
    expect(force!.pds).toBe(1);
    expect(formatDefenderUnits(force!.otherStructures)).toBe("1 Space Dock");
    expect(force!.guns).toEqual([{ unit: "pds", count: 1, hitsOn: 6, dice: 1 }]);
    expect(defense.spaceCannonDefense).toEqual({ dice: 1, expectedHits: 0.5 });
  });

  it("uses the defender's PDS II and reports L4 Disruptors silencing Space Cannon Defense", () => {
    const units = [unit("rival", "pds", "jord"), unit("rival", "pds", "jord")];
    const upgraded = planetDefense(board(units), "18", "jord", "me", {
      rival: seat("rival", "hacan", ["pds2"]),
    });
    expect(upgraded.forces[0]!.groundTotal).toBe(0);
    expect(upgraded.forces[0]!.guns).toEqual([{ unit: "pds", count: 2, hitsOn: 5, dice: 1 }]);
    expect(upgraded.spaceCannonDefense).toEqual({ dice: 2, expectedHits: 1.2 });
    const silenced = planetDefense(board(units), "18", "jord", "me", {
      me: seat("me", "letnev", ["l4"]),
      rival: seat("rival", "hacan"),
    });
    expect(silenced.spaceCannonDefense?.silencedBy).toBe("L4 Disruptors");
  });

  it("has no defenders or Space Cannon Defense on a planet holding only the invader's units", () => {
    const defense = planetDefense(board([unit("me", "pds", "jord")]), "18", "jord", "me");
    expect(defense).toEqual({ forces: [], spaceCannonDefense: null });
  });
});
