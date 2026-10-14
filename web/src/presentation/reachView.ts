import type { ReachOrigin, ReachShip } from "../protocol/types.ts";
import type { MovementVector } from "./boardPresentation.ts";

/** One owner's ships in one origin system that could move into the selected system. */
export interface ReachEntry {
  /** Stable key for selection: `<owner>|<origin>`. */
  key: string;
  owner: string;
  origin: string;
  ships: ReachShip[];
  total: number;
}

export interface ReachGroups {
  own: ReachEntry[];
  enemy: ReachEntry[];
}

export function reachEntryKey(owner: string, origin: string): string {
  return `${owner}|${origin}`;
}

/**
 * Splits the engine's answer into the viewer's own ships and everybody else's. A spectator (no
 * seat) has no own ships, so every entry lands under the other players. Owners keep the order the
 * server sent (seating order); origins within an owner keep the server's id order.
 */
export function groupReach(origins: readonly ReachOrigin[], viewerSeat: string | null): ReachGroups {
  const groups: ReachGroups = { own: [], enemy: [] };
  for (const entry of origins) {
    const ships = entry.ships.filter((ship) => ship.count > 0);
    if (ships.length === 0) continue;
    const reach: ReachEntry = {
      key: reachEntryKey(entry.owner, entry.origin),
      owner: entry.owner,
      origin: entry.origin,
      ships,
      total: ships.reduce((sum, ship) => sum + ship.count, 0),
    };
    (viewerSeat !== null && entry.owner === viewerSeat ? groups.own : groups.enemy).push(reach);
  }
  return groups;
}

/**
 * The arrows for the map: one per owner and origin system, towards the queried system. When an
 * entry is picked the others stay but fade.
 */
export function reachVectors(
  groups: ReachGroups,
  destination: string,
  centers: ReadonlyMap<string, { x: number; y: number }>,
  focusKey: string | null,
): MovementVector[] {
  const toCenter = centers.get(destination);
  if (!toCenter) return [];
  const vectors: MovementVector[] = [];
  for (const tone of ["own", "enemy"] as const) {
    for (const entry of groups[tone]) {
      const fromCenter = centers.get(entry.origin);
      if (!fromCenter || entry.origin === destination) continue;
      vectors.push({
        fromSystemId: entry.origin,
        toSystemId: destination,
        fromCenter,
        toCenter,
        unitCount: entry.total,
        optionIds: [],
        tone,
        dimmed: focusKey !== null && focusKey !== entry.key,
      });
    }
  }
  return vectors;
}
