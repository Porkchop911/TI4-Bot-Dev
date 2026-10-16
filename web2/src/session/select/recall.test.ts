import { describe, expect, it } from "vitest";
import type { Choice, SessionUpdate } from "../wire";
import {
  chooseRecallSystem,
  clearLastRecall,
  clearRecallPart,
  countRecallPools,
  selectRecall,
} from "./recall";

/** Every arrangement of `total` tokens with at least 2 in the fleet pool, as the engine offers them. */
const arrangements = (total: number) =>
  Array.from({ length: total + 1 }, (_, t) =>
    Array.from({ length: total - t + 1 }, (_, f) => [t, f, total - t - f]),
  )
    .flat()
    .filter(([, f]) => f >= 2)
    .map(([t, f, s]) => `${t}|${f}|${s}`);

// The recall of Warfare's primary as the engine asks it (the scenario "strategy_warfare"): Sol has
// 3 · 3 · 2 on its sheet and its command token on #29.
const recall: Choice = {
  player: "a",
  prompt: "recall a command token",
  options: [{ id: "29", kind: "recall", label: "29", payload: {} }],
  details: {
    redistribution: {
      pools: { tactic: 3, fleet: 3, strategic: 2 },
      gained: 1,
      total: 9,
      gain: { tactic: "tactic_tokens", fleet: "fleet_tokens", strategic: "strategic_tokens" },
      arrangements: arrangements(9),
    },
  },
};
const update = {
  viewer: { role: "player", seat: "a" },
  view: {
    players: [{ id: "a" }],
    board: { map_tiles: [{ system_id: "29", label: "Qucenn/Rarron" }] },
  },
} as unknown as SessionUpdate;

describe("selectRecall", () => {
  it("is null for any other question", () => {
    const other: Choice = {
      ...recall,
      options: [{ id: "29", kind: "system", label: "29", payload: {} }],
    };
    expect(selectRecall(update, other, null)).toBeNull();
  });

  it("has the token on the map and the pools with the gained token to place", () => {
    const view = selectRecall(update, recall, null)!;
    expect(view.blocks.map((block) => block.kind === "card" && block.title)).toEqual([
      "Command token",
      "Command pools",
    ]);
    expect(view.task).toMatchObject({ target: "system", values: { "29": 0 }, chosen: {} });
    expect(view.problem).toBe("Choose a system on the map.");
    expect(view.answers).toBeNull();
    expect(view.changed).toBe(false);
    const pools = view.blocks[1];
    expect(pools.kind === "card" && pools.aside).toBe("8 / 9 placed");
  });

  it("answers the system, the gain into a pool that grows, and the arrangement", () => {
    let draft = chooseRecallSystem(recall, null, "29")!;
    expect(selectRecall(update, recall, draft)!.problem).toBe("Place 1 more token.");
    draft = countRecallPools(recall, draft, "pools.f", 4)!;
    const view = selectRecall(update, recall, draft)!;
    expect(view.problem).toBeNull();
    expect(view.note).toBe("Qucenn/Rarron · #29 · Tactic 3 · Fleet 3 → 4 · Strategy 2");
    expect(view.answers).toEqual(["29", "fleet_tokens", "3|4|2"]);
    expect(view.label).toBe("Resolve primary");
    // An arrangement that the game does not offer is not sent.
    const thin = countRecallPools(
      recall,
      { system: "29", pools: { tactic: 8, fleet: 0, strategic: 0 } },
      "pools.s",
      1,
    )!;
    expect(selectRecall(update, recall, thin)!.answers).toBeNull();
  });

  it("clears the pools, then the system; Change gives the system back", () => {
    const draft = countRecallPools(recall, chooseRecallSystem(recall, null, "29"), "pools.t", 4)!;
    expect(clearRecallPart(recall, draft, "system")).toEqual({ ...draft, system: null });
    const once = clearLastRecall(recall, draft)!;
    expect(once).toEqual({ system: "29", pools: null });
    expect(clearLastRecall(recall, once)).toEqual({ system: null, pools: null });
    expect(chooseRecallSystem(recall, null, "30")).toBeNull();
  });
});
