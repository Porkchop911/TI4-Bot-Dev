import { describe, expect, it } from "vitest";
import { describeExploreCard } from "./exploreReward.ts";

const reward = (card: string) => ({
  context: { subtype: `${card}_choose_reward`, source: { Content: card } } as never,
});

describe("exploration card rewards", () => {
  it("shows the card as printed, found by its alias or its underscored name", () => {
    const byAlias = describeExploreCard(reward("lf1"));
    expect(byAlias?.name).toBe("Local Fabricators");
    expect(byAlias?.text.length).toBeGreaterThan(0);
    const byName = describeExploreCard(reward("abandoned_warehouses"));
    expect(byName?.name).toBe("Abandoned Warehouses");
    expect(byName?.type).toBe("Industrial");
  });

  it("falls back to the subtype when the source is missing", () => {
    expect(describeExploreCard({ context: { subtype: "lf1_choose_reward" } as never })?.name).toBe(
      "Local Fabricators",
    );
  });

  it("is nothing for other decisions or unknown cards", () => {
    expect(describeExploreCard({ context: { subtype: "spy_pick_player" } as never })).toBeNull();
    expect(describeExploreCard(reward("no_such_card"))).toBeNull();
  });
});
