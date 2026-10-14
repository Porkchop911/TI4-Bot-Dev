import { describe, expect, it } from "vitest";
import { groupReach, reachVectors, reachEntryKey } from "./reachView.ts";
import type { ReachOrigin } from "../protocol/types.ts";

const ship = (type_id: string, count: number) => ({
  type_id,
  count,
  damaged: 0,
  gravity_drive: 0,
  ionian: 0,
});

const origins: ReachOrigin[] = [
  { owner: "me", origin: "27", ships: [ship("cruiser", 2), ship("destroyer", 1)] },
  { owner: "me", origin: "30", ships: [ship("carrier", 1)] },
  { owner: "them", origin: "41", ships: [ship("dreadnought", 1)] },
  { owner: "them", origin: "44", ships: [ship("cruiser", 0)] },
];

describe("reach view", () => {
  it("splits the viewer's ships from everybody else's and drops empty rows", () => {
    const groups = groupReach(origins, "me");
    expect(groups.own.map((e) => e.key)).toEqual([reachEntryKey("me", "27"), reachEntryKey("me", "30")]);
    expect(groups.own[0].total).toBe(3);
    expect(groups.enemy.map((e) => e.origin)).toEqual(["41"]);
  });

  it("puts every ship under the others for a spectator", () => {
    const groups = groupReach(origins, null);
    expect(groups.own).toEqual([]);
    expect(groups.enemy).toHaveLength(3);
  });

  it("draws one arrow per owner and origin, coloured by tone, fading the unpicked ones", () => {
    const centers = new Map([
      ["18", { x: 0, y: 0 }],
      ["27", { x: 10, y: 0 }],
      ["30", { x: 0, y: 10 }],
      ["41", { x: -10, y: 0 }],
    ]);
    const groups = groupReach(origins, "me");
    const all = reachVectors(groups, "18", centers, null);
    expect(all.map((v) => [v.tone, v.fromSystemId, v.unitCount, v.dimmed])).toEqual([
      ["own", "27", 3, false],
      ["own", "30", 1, false],
      ["enemy", "41", 1, false],
    ]);
    const focused = reachVectors(groups, "18", centers, reachEntryKey("them", "41"));
    expect(focused.map((v) => v.dimmed)).toEqual([true, true, false]);
    expect(reachVectors(groups, "99", centers, null)).toEqual([]);
  });
});
