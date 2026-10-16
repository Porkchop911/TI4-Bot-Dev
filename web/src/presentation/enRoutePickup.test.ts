import { describe, expect, it } from "vitest";
import type { BoardView } from "../protocol/types.ts";
import { enRoutePickupSystems, predictRouteBetween } from "./enRoutePickup.ts";

const tile = (system_id: string, q: number, r: number, extra = {}) => ({
  system_id,
  label: `#${system_id}`,
  q,
  r,
  ...extra,
});
const sys = (
  id: string,
  units: BoardView["systems"][string]["units"] = [],
  tokens: string[] = [],
) => ({ system_id: id, command_tokens: tokens, planets: {}, units });

// 24 - 30 - 18 in a row; 31 sits beside them, above 30
const board = (): BoardView => ({
  map_tiles: [tile("24", 0, 0), tile("30", 1, 0), tile("18", 2, 0), tile("31", 1, -1)],
  systems: {
    "24": sys("24"),
    "30": sys("30", [{ owner: "p1", unit_type: "infantry", planet: "mid", damaged: false }]),
    "18": sys("18"),
    "31": sys("31"),
  },
});

describe("predictRouteBetween", () => {
  it("lists the systems strictly between origin and destination", () => {
    expect(predictRouteBetween(board(), "24", "18", "p1")).toEqual(["30"]);
  });

  it("is empty for adjacent systems, a missing map, and an unknown system", () => {
    expect(predictRouteBetween(board(), "24", "30", "p1")).toEqual([]);
    expect(predictRouteBetween({ systems: {} }, "24", "18", "p1")).toEqual([]);
    expect(predictRouteBetween(board(), "99", "18", "p1")).toEqual([]);
  });

  it("goes around a system that holds another player's ships", () => {
    const b = board();
    b.systems["30"].units.push({ owner: "p2", unit_type: "cruiser", damaged: false });
    // (0,0) -> 31 (1,-1) -> needs (2,-1) to reach (2,0): no route yet
    expect(predictRouteBetween(b, "24", "18", "p1")).toEqual([]);
    b.map_tiles!.push(tile("32", 2, -1));
    expect(predictRouteBetween(b, "24", "18", "p1")).toEqual(["31", "32"]);
  });

  it("does not pass through an anomaly a ship cannot stop in", () => {
    const b = board();
    b.map_tiles![1] = tile("30", 1, 0, { anomalies: ["supernova"] });
    expect(predictRouteBetween(b, "24", "18", "p1")).toEqual([]);
  });

  it("breaks ties like the engine: neighbours in system-id order", () => {
    const b: BoardView = {
      map_tiles: [tile("24", 0, 0), tile("40", 1, 0), tile("35", 0, 1), tile("18", 1, 1)],
      systems: {},
    };
    expect(predictRouteBetween(b, "24", "18", "p1")).toEqual(["35"]);
  });
});

describe("enRoutePickupSystems", () => {
  it("drops a system holding the player's own command token (95.5)", () => {
    const b = board();
    expect(enRoutePickupSystems(b, "24", "18", "p1")).toEqual(["30"]);
    b.systems["30"].command_tokens = ["p1"];
    expect(enRoutePickupSystems(b, "24", "18", "p1")).toEqual([]);
  });
});
