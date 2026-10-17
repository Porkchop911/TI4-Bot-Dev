import { describe, expect, it } from "vitest";
import { CONTENT } from "../protocol/catalogStore.ts";
import {
  EVENT_SECRETS,
  SECRET_PROGRESS_UNITS,
  secretNoProgressNote,
  secretUnreachableNote,
  secretProgressDisplay,
} from "./secretProgress.ts";

describe("secret objective progress wording", () => {
  it("words a count as have / threshold plus what counts", () => {
    const shown = secretProgressDisplay("ctr", { have: 4, threshold: 6, satisfied: false });
    expect(shown?.text).toBe("4 / 6 systems with at least one of your ships");
    expect(shown?.percent).toBe(67);
    expect(secretProgressDisplay("eh", { have: 15, threshold: 12, satisfied: true })?.percent).toBe(
      100,
    );
  });

  it("reminds that Strengthen Bonds counts play-area notes only, not the hand", () => {
    const shown = secretProgressDisplay("sb", { have: 0, threshold: 1, satisfied: false });
    expect(shown?.hint).toMatch(/face-up in your play area/);
    expect(shown?.hint).toMatch(/hand do not/);
    expect(secretProgressDisplay("ctr", { have: 1, threshold: 6, satisfied: false })?.hint).toBeNull();
  });

  it("has no bar without a count", () => {
    expect(secretProgressDisplay("ctr", undefined)).toBeNull();
    expect(secretNoProgressNote("pe")).toMatch(/event/);
    expect(secretNoProgressNote("ctr")).toBeNull();
  });

  it("covers every base secret as either a count or an event", () => {
    const aliases = Object.keys(CONTENT.secretObjectives).filter(
      (id) => id.length <= 4,
    );
    expect(aliases.length).toBe(40);
    for (const alias of aliases) {
      const counted = alias in SECRET_PROGRESS_UNITS;
      const event = EVENT_SECRETS.has(alias);
      expect(counted !== event, alias).toBe(true);
    }
  });
});

describe("secrets the map cannot serve", () => {
  const tile = (system_id: string, planets: { id: string; legendary?: boolean }[], special_area?: string) => ({
    system_id,
    label: system_id,
    q: 0,
    r: 0,
    special_area,
    planets: planets.map((p) => ({ ...p, label: p.id, resources: 0, influence: 0 })),
  });

  it("says so for Seize an Icon when no legendary planet is on the map", () => {
    const board = { systems: {}, map_tiles: [tile("20", [{ id: "vefutii" }]), tile("82a", [{ id: "lockedmallice", legendary: true }], "nexus")] };
    expect(secretUnreachableNote("sai", board)).toMatch(/No legendary planet is on this map/);
  });

  it("is silent when a legendary planet is on the map, for other secrets, and without a map", () => {
    const withOne = { systems: {}, map_tiles: [tile("65", [{ id: "primor", legendary: true }])] };
    expect(secretUnreachableNote("sai", withOne)).toBeNull();
    expect(secretUnreachableNote("ctr", { systems: {}, map_tiles: [tile("20", [])] })).toBeNull();
    expect(secretUnreachableNote("sai", null)).toBeNull();
    expect(secretUnreachableNote("sai", { systems: {} })).toBeNull();
  });
});
