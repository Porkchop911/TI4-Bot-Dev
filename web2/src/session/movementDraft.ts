// The staged movement: which ships move and what each loads. Nothing of it is sent before
// "Move fleet". The limits (who can move, what a ship can load, how many are there) are facts of
// the engine; this file only counts what is staged against them.
import type { MovementFacts, ShipFact } from "./wire";

/** A ship of the facts: the system it is in and its place there. */
export type ShipId = string;
export const shipId = (ship: Pick<ShipFact, "origin" | "index">): ShipId =>
  `${ship.origin}|${ship.index}`;

/** For each ship that moves: how many units of each cargo pool it loads. */
export type MovementDraft = Record<ShipId, Record<number, number>>;

export const shipOf = (facts: MovementFacts, id: ShipId) =>
  facts.ships.find((ship) => shipId(ship) === id);

/** How many units a ship has in its hold. */
export const holdLoad = (draft: MovementDraft, id: ShipId) =>
  Object.values(draft[id] ?? {}).reduce((sum, count) => sum + count, 0);

/** How many units of a pool the staged ships take. */
export const poolTaken = (draft: MovementDraft, pool: number) =>
  Object.values(draft).reduce((sum, hold) => sum + (hold[pool] ?? 0), 0);

export const poolLeft = (facts: MovementFacts, draft: MovementDraft, pool: number) =>
  (facts.cargo[pool]?.count ?? 0) - poolTaken(draft, pool);

/** Another ship that already has the one boost that this ship needs. Null when it is free. */
export function boostHolder(
  facts: MovementFacts,
  draft: MovementDraft,
  ship: ShipFact,
): { boost: "gravity_drive" | "ionian"; holder: ShipFact } | null {
  for (const boost of ["gravity_drive", "ionian"] as const) {
    if (!ship.move?.[boost]) {
      continue;
    }
    const holder = facts.ships.find(
      (other) => other !== ship && other.move?.[boost] && shipId(other) in draft,
    );
    if (holder) {
      return { boost, holder };
    }
  }
  return null;
}

/** Whether the ship can be staged now. */
export const canMove = (facts: MovementFacts, draft: MovementDraft, ship: ShipFact) =>
  !!ship.move &&
  (!ship.move.gravity_drive || facts.gravity_drive) &&
  (!ship.move.ionian || facts.ionian) &&
  !boostHolder(facts, draft, ship);

/** Stages a ship, or takes it back with its hold. */
export function setShip(
  facts: MovementFacts,
  draft: MovementDraft,
  id: ShipId,
  moves: boolean,
): MovementDraft {
  const ship = shipOf(facts, id);
  if (!moves) {
    const { [id]: _, ...rest } = draft;
    return rest;
  }
  return !ship || id in draft || !canMove(facts, draft, ship) ? draft : { ...draft, [id]: {} };
}

/** Sets how many units of one pool a moving ship loads, within its hold and what is left. */
export function setLoad(
  facts: MovementFacts,
  draft: MovementDraft,
  id: ShipId,
  pool: number,
  count: number,
): MovementDraft {
  const ship = shipOf(facts, id);
  const hold = draft[id];
  if (!ship || !hold || !ship.loads?.includes(pool)) {
    return draft;
  }
  const now = hold[pool] ?? 0;
  const room = ship.capacity - holdLoad(draft, id) + now;
  const next = Math.max(0, Math.min(count, room, now + poolLeft(facts, draft, pool)));
  const { [pool]: _, ...others } = hold;
  return { ...draft, [id]: next ? { ...others, [pool]: next } : others };
}

/** Loads one pool on every moving ship of a system, until the holds are full or none is left. */
export function fill(
  facts: MovementFacts,
  draft: MovementDraft,
  origin: string,
  pool: number,
): MovementDraft {
  let next = draft;
  for (const ship of facts.ships) {
    const id = shipId(ship);
    if (ship.origin === origin && id in next) {
      next = setLoad(facts, next, id, pool, ship.capacity);
    }
  }
  return next;
}

/** Takes back everything that leaves one system. */
export const resetOrigin = (draft: MovementDraft, origin: string): MovementDraft =>
  Object.fromEntries(Object.entries(draft).filter(([id]) => !id.startsWith(`${origin}|`)));

// The ids of the staged values, as the sheet of a system sends them back in `setCount`.
export const shipKey = (id: ShipId) => `ship:${id}`;
export const loadKey = (id: ShipId, pool: number) => `load:${id}:${pool}`;
export const sourceId = (pool: number) => `pool:${pool}`;
export const poolOfSource = (source: string | undefined) =>
  source?.startsWith("pool:") ? Number(source.slice(5)) : null;

/** A `setCount` of the movement sheet. Any other key leaves the draft as it is. */
export function setCount(
  facts: MovementFacts,
  draft: MovementDraft,
  key: string,
  value: number,
): MovementDraft {
  if (key.startsWith("ship:")) {
    return setShip(facts, draft, key.slice(5), value > 0);
  }
  const [, id, pool] = /^load:(.+):(\d+)$/.exec(key) ?? [];
  return id ? setLoad(facts, draft, id, Number(pool), value) : draft;
}
