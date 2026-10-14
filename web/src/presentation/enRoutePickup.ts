import type { BoardView } from "../protocol/types.ts";

// 95.1: a transporting ship "can pick up and transport units from the active system, the system it
// started its movement in, and each system it moves through". The engine opens the hold after the
// ship is chosen, with every system of the route it computed (crates/ti4-engine/src/transit.rs
// `CargoWindow::for_ship`), but the move offer does not say which systems that route crosses. The
// UI therefore predicts the route the way `MovementRules::path_from_ship` finds it: breadth first
// over hex neighbours (and shared wormholes), neighbours in system-id string order, the first route
// to reach the active system wins. A wrong prediction is harmless: the engine only offers pickups
// on its own route, so a staged pickup that is not on it makes the batch fail before anything is
// applied.

const DIRECTIONS: [number, number][] = [
  [1, 0],
  [1, -1],
  [0, -1],
  [-1, 0],
  [-1, 1],
  [0, 1],
];

// Anomalies a ship cannot stop in on its way through (59.1a nebula, supernova, asteroid field).
const IMPASSABLE = /nebula|supernova|asteroid/i;

/**
 * The systems strictly between `origin` and `destination` on the predicted route, in route order.
 * Empty when the map has no coordinates or no route is found.
 *
 * `actor`'s own ships never block; another player's units in a space area do (58.4b).
 */
export function predictRouteBetween(
  board: BoardView | undefined,
  origin: string,
  destination: string,
  actor: string,
): string[] {
  const tiles = board?.map_tiles;
  if (!tiles?.length || origin === destination) return [];
  const byId = new Map(tiles.map((tile) => [tile.system_id, tile]));
  const byCoord = new Map(
    tiles
      .filter((tile) => !tile.special_area)
      .map((tile) => [`${tile.q},${tile.r}`, tile]),
  );
  if (!byId.has(origin) || !byId.has(destination)) return [];

  const passable = (id: string) => {
    const tile = byId.get(id);
    if (!tile || tile.hyperlane) return false;
    if ((tile.anomalies ?? []).some((a) => IMPASSABLE.test(a))) return false;
    return !(board?.systems?.[id]?.units ?? []).some(
      (u) => !u.planet && u.owner !== actor,
    );
  };
  const neighbours = (id: string): string[] => {
    const tile = byId.get(id);
    if (!tile) return [];
    const found = new Set<string>();
    if (!tile.special_area) {
      for (const [dq, dr] of DIRECTIONS) {
        const next = byCoord.get(`${tile.q + dq},${tile.r + dr}`);
        if (next) found.add(next.system_id);
      }
    }
    for (const kind of tile.wormholes ?? []) {
      for (const other of tiles) {
        if (other.system_id !== id && (other.wormholes ?? []).includes(kind)) {
          found.add(other.system_id);
        }
      }
    }
    return [...found].sort();
  };

  const seen = new Set<string>([origin]);
  const queue: string[][] = [[origin]];
  while (queue.length) {
    const route = queue.shift()!;
    const current = route[route.length - 1];
    for (const next of neighbours(current)) {
      if (next === destination) return route.slice(1);
      if (seen.has(next) || !passable(next)) continue;
      seen.add(next);
      queue.push([...route, next]);
    }
  }
  return [];
}

/**
 * Systems whose units a ship from `origin` could still pick up: on the predicted route, without a
 * command token of `actor` (95.5; the active system is exempt but is not "en route").
 */
export function enRoutePickupSystems(
  board: BoardView | undefined,
  origin: string,
  destination: string,
  actor: string,
): string[] {
  return predictRouteBetween(board, origin, destination, actor).filter(
    (id) => !(board?.systems?.[id]?.command_tokens ?? []).includes(actor),
  );
}

/**
 * Units that are transported without using a capacity slot. The engine decides this per unit with
 * the faction hook `MovementHooks::free_cargo`; the only user is the Argent Flight's Aerie Sentinel
 * (crates/ti4-engine/src/factions/argent.rs `free_cargo`: `type_id == "argent_mech"`). The hold
 * payload does not carry the flag, so the UI keys on the unit type id the board reports.
 */
export function ridesFree(unitType: string): boolean {
  return unitType === "argent_mech";
}

/**
 * 95.5 (engine: crates/ti4-engine/src/transit.rs `loadable_by`): fighters and ground forces cannot
 * be picked up from a system that holds one of the actor's own command tokens, unless it is the
 * active system. Ordinarily unreachable (58.4c keeps a ship from leaving such a system), but the
 * Dominus Orb suspends that, and the garrison must still stay behind. The Nomad hero's
 * `ignores_command_tokens` exemption is not visible to the client.
 */
export function originCargoBlockedByToken(
  board: BoardView | undefined,
  system: string,
  actor: string,
  activeSystem: string | null | undefined,
): boolean {
  if (system === activeSystem) return false;
  return (board?.systems?.[system]?.command_tokens ?? []).includes(actor);
}
