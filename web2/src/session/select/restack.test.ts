import { describe, expect, it } from "vitest";
import type { Choice } from "../wire";
import { countRestack, selectRestack } from "./restack";

const NAMES = ["tactic", "fleet", "strategy"];

/** Predictive Intelligence as the engine asks it: one move of one token, and "done". */
const restack = (pools: [number, number, number]): Choice => ({
  player: "A",
  prompt: "Predictive Intelligence: redistribute command tokens",
  options: [
    ...NAMES.flatMap((source, index) =>
      pools[index] > 0
        ? NAMES.filter((destination) => destination !== source).map((destination) => ({
            id: `${source}|${destination}`,
            kind: "redistribute",
            label: `move 1 token from ${source} to ${destination}`,
            payload: {},
          }))
        : [],
    ),
    { id: "done", kind: "decline", label: "finish redistribution", payload: {} },
  ],
  details: {
    kind: "command_tokens",
    mode: "restack",
    pools: { tactic: pools[0], fleet: pools[1], strategic: pools[2] },
    total: pools[0] + pools[1] + pools[2],
    reinforcements: 8,
  },
});

describe("selectRestack", () => {
  it("is null for any other question", () => {
    const other = restack([3, 3, 2]);
    expect(
      selectRestack({ ...other, details: { ...other.details, mode: "gain" } }, null),
    ).toBeNull();
    expect(
      selectRestack({ ...other, options: other.options.filter((o) => o.id === "done") }, null),
    ).toBeNull();
  });

  it("is the pool editor from the sheet of now, and declines when nothing moves", () => {
    const view = selectRestack(restack([3, 3, 2]), null)!;
    const block = view.blocks[0];
    expect(block.kind === "card" && block.title).toBe("Command pools");
    expect(block.kind === "card" && block.aside).toBe("8 / 8 placed");
    expect(view.answers).toEqual(["done"]);
    expect(view.label).toBe("Skip");
    expect(view.changed).toBe(false);
    expect(view.onMap).toBe(true);
  });

  it("answers one move for each token, from the pools that shrink to those that grow", () => {
    const choice = restack([3, 3, 2]);
    let draft = countRestack(choice, null, "pools.t", 1)!;
    expect(selectRestack(choice, draft)!.problem).toBe("Place 2 more tokens.");
    expect(selectRestack(choice, draft)!.answers).toBeNull();
    draft = countRestack(choice, draft, "pools.f", 4)!;
    draft = countRestack(choice, draft, "pools.s", 3)!;
    const view = selectRestack(choice, draft)!;
    expect(view.problem).toBeNull();
    expect(view.note).toBe("Tactic 3 → 1 · Fleet 3 → 4 · Strategy 2 → 3");
    expect(view.answers).toEqual(["tactic|fleet", "tactic|strategy", "done"]);
    expect(view.label).toBe("Redistribute");
  });

  it("moves into a pool that is empty now", () => {
    const choice = restack([4, 2, 0]);
    const draft = { tactic: 2, fleet: 2, strategic: 2 };
    expect(selectRestack(choice, draft)!.answers).toEqual([
      "tactic|strategy",
      "tactic|strategy",
      "done",
    ]);
  });
});
