import { describe, expect, it } from "vitest";
import type { Choice } from "../wire";
import { countPools, selectTokens } from "./tokens";

/** Every arrangement of `total` tokens with at least `fleet` in the fleet pool, as the engine offers them. */
const arrangements = (total: number, fleet = 2) =>
  Array.from({ length: total + 1 }, (_, t) =>
    Array.from({ length: total - t + 1 }, (_, f) => [t, f, total - t - f]),
  )
    .flat()
    .filter(([, f]) => f >= fleet)
    .map(([t, f, s]) => ({
      id: `${t}|${f}|${s}`,
      kind: "redistribute",
      label: `Tactic ${t} / fleet ${f} / strategy ${s}`,
      payload: {},
    }));

const question = (
  pools: [number, number, number],
  gained: number,
  mode = "redistribute",
): Choice => {
  const total = pools[0] + pools[1] + pools[2] + gained;
  return {
    player: "A",
    prompt: "redistribute your command tokens",
    options: arrangements(total),
    details: {
      kind: "command_tokens",
      mode,
      pools: { tactic: pools[0], fleet: pools[1], strategic: pools[2] },
      total,
      ...(gained ? { gained } : {}),
      reinforcements: 8,
    },
  };
};

const counters = (view: NonNullable<ReturnType<typeof selectTokens>>) =>
  view.block.rows.map((row) => (row.control?.kind === "counter" ? row.control.counter : null));

describe("selectTokens", () => {
  it("is null for a restack and for any other question", () => {
    expect(selectTokens(question([3, 3, 2], 0, "restack"), null)).toBeNull();
    const other: Choice = {
      player: "A",
      prompt: "choose",
      options: [{ id: "x", kind: "pick", label: "X", payload: {} }],
    };
    expect(selectTokens(other, null)).toBeNull();
    const gain = question([3, 3, 2], 0, "gain");
    expect(selectTokens(gain, null)).toBeNull();
  });

  it("shows the gained tokens as tokens left to place", () => {
    const view = selectTokens(question([3, 3, 0], 2), null)!;
    expect(view.block.title).toBe("Command pools");
    expect(view.block.aside).toBe("6 / 8 placed");
    expect(view.block.bad).toBe(true);
    expect(view.staged).toBeNull();
    expect(view.note).toBe("Place 2 more tokens");
    expect(view.block.rows.map((row) => row.tokens)).toEqual([
      { now: 3, after: 3, up: false },
      { now: 3, after: 3, up: true },
      { now: 0, after: 0, up: false },
    ]);
  });

  it("says the fleet supply bonus of the engine on the fleet pool only", () => {
    const plain = selectTokens(question([3, 3, 2], 0), null)!;
    expect(plain.block.rows.map((row) => row.subtitle)).toEqual([undefined, undefined, undefined]);
    const letnev = question([3, 3, 2], 0);
    letnev.details = {
      ...letnev.details,
      pools: { tactic: 3, fleet: 3, strategic: 2, fleet_supply_bonus: 2 },
    };
    const view = selectTokens(letnev, null)!;
    expect(view.block.rows.map((row) => row.subtitle)).toEqual([undefined, "+2 supply", undefined]);
    expect(view.block.rows[1].hint).toContain("2 more non-fighter ships");
    // A staged arrangement keeps the bonus on the fleet pool, and its id is the pools only.
    const staged = countPools(letnev, null, "pools.f", 4)!;
    expect(staged.pools.fleet).toBe(4);
    expect(selectTokens(letnev, staged.pools)!.block.rows[1].subtitle).toBe("+2 supply");
  });

  it("takes the bounds of the counters from the offered arrangements", () => {
    const view = selectTokens(question([3, 3, 0], 2), null)!;
    expect(counters(view).map((counter) => [counter?.id, counter?.min, counter?.max])).toEqual([
      ["pools.t", 0, 5],
      ["pools.f", 2, 5],
      ["pools.s", 0, 2],
    ]);
  });

  it("stages an arrangement only when it is offered", () => {
    const choice = question([3, 3, 0], 2);
    const once = countPools(choice, null, "pools.s", 1)!;
    expect(once).toEqual({ pools: { tactic: 3, fleet: 3, strategic: 1 }, staged: null });
    const twice = countPools(choice, once.pools, "pools.f", 4)!;
    expect(twice.staged).toBe("3|4|1");
    const view = selectTokens(choice, twice.pools)!;
    expect(view.staged).toBe("3|4|1");
    expect(view.block.aside).toBe("8 / 8 placed");
    expect(view.block.bad).toBe(false);
    expect(view.note).toBe("Tactic 3 · Fleet 3 → 4 · Strategy 0 → 1");
    // A full sheet that the game does not offer stages nothing.
    const thin = countPools(choice, { tactic: 6, fleet: 2, strategic: 0 }, "pools.f", 1)!;
    expect(thin.staged).toBeNull();
    expect(countPools(choice, null, "buy", 1)).toBeNull();
  });

  it("starts a redistribution with no gain at an offered arrangement", () => {
    const view = selectTokens(question([3, 3, 2], 0), null)!;
    expect(view.block.aside).toBe("8 / 8 placed");
    expect(view.staged).toBe("3|3|2");
    expect(view.note).toBe("Tactic 3 · Fleet 3 · Strategy 2");
    expect(counters(view).map((counter) => counter?.max)).toEqual([3, 3, 2]);
  });
});
