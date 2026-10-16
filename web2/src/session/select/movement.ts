// The movement step from the facts of the engine and the staged movement: what can leave each
// system, what the ships load, what arrives, and the marks of all this on the board.
import type {
  BattleTableView,
  CargoSourceView,
  MoveShipView,
  MovementView,
  OriginView,
  RouteView,
  ShipHoldView,
  ShipUnitView,
  SpaceOddsView,
  TileView,
  TrailView,
  UnitType,
} from "../../model";
import { plural } from "../../model";
import {
  type MovementDraft,
  boostHolder,
  canMove,
  holdLoad,
  loadKey,
  poolLeft,
  shipId,
  shipKey,
  sourceId,
} from "../movementDraft";
import { type KeptOdds, oddsRequest } from "../movementOdds";
import type {
  BoardTileView,
  CannonFact,
  Catalog,
  CargoPool,
  MovementFacts,
  PublicRoll,
  SessionUpdate,
  ShipFact,
  Unloadable,
  UnloadableCargo,
} from "../wire";
import { type ShipsOf, cannonPreview, cannonTable, removedUnits, riftRows } from "./movementRolls";
import { combatOdds } from "./odds";
import { WORMHOLES, byUnitOrder, forceOf, isShip, unitKey, unitLabel, unitType } from "./names";

const RIFT_RULE = "Each ship that leaves a gravity rift rolls: destroyed on 1–3, with its cargo";

/** The icon of a unit. A unit that the table of names does not have is shown as a cruiser. */
const typeOf = (unit: string): UnitType => unitType(unit) ?? "cruiser";
/** The printed name of a unit: "Carrier II", "Infantry I (damaged)". */
const nameOf = (plan: Pick<Plan, "printed">, unit: string, damaged = false, count = 1) =>
  unitLabel(unit, plan.printed, count) + (damaged ? " (damaged)" : "");

/** Units by their printed unit (`unitKey`): one of their ids, and how many. */
type Counted = Map<string, { id: string; count: number }>;
function countUnits(units: Iterable<[id: string, count: number]>, printed: Catalog | null) {
  const counted: Counted = new Map();
  for (const [id, count] of units) {
    const key = unitKey(id, printed);
    counted.set(key, { id, count: (counted.get(key)?.count ?? 0) + count });
  }
  return counted;
}

/** What the selectors of the movement share: the names of the map, the facts, the draft. */
interface Plan {
  facts: MovementFacts;
  draft: MovementDraft;
  tiles: Map<string, BoardTileView>;
  viewer: string;
  /** The systems that the player marked as done. */
  handled: string[];
  /** The printed names of the units. */
  printed: Catalog | null;
}

function planOf(
  update: SessionUpdate,
  facts: MovementFacts,
  draft: MovementDraft,
  handled: string[] = [],
  printed: Catalog | null = null,
): Plan {
  return {
    facts,
    draft,
    handled,
    printed,
    tiles: tilesOf(update),
    viewer: update.viewer.role === "player" ? update.viewer.seat : "",
  };
}

/** The systems of the board by id. */
export const tilesOf = (update: SessionUpdate): Map<string, BoardTileView> =>
  new Map((update.view.board.map_tiles ?? []).map((tile) => [tile.system_id, tile]));

export const systemLabel = (tiles: Map<string, BoardTileView>, id: string) =>
  `${tiles.get(id)?.label ?? "System"} · #${id}`;

/** "via Tar’Mann · #23 · β wormhole": the systems between the start and the active system. */
function viaText(plan: Plan, ship: ShipFact) {
  const move = ship.move!;
  const inner = move.path.slice(1, -1);
  return [
    inner.length ? `via ${inner.map((id) => systemLabel(plan.tiles, id)).join(", ")}` : "Direct",
    ...(move.wormholes ?? []).map((kind) => `${WORMHOLES[kind] ?? kind} wormhole`),
    ...(move.ionian ? ["Ionian Fuel Refinery"] : []),
  ].join(" · ");
}

const BLOCKED = {
  command_token: "Your command token is in this system",
  range: "Out of range",
};

/** Why a ship cannot be staged now. Null when it can. */
function reasonOf(plan: Plan, ship: ShipFact): string | null {
  if (ship.blocked) {
    return BLOCKED[ship.blocked];
  }
  if (!ship.move || canMove(plan.facts, plan.draft, ship)) {
    return null;
  }
  const taken = boostHolder(plan.facts, plan.draft, ship);
  const boost = ship.move.gravity_drive && (!plan.facts.gravity_drive || taken?.boost !== "ionian");
  const name = boost ? "Gravity Drive" : "the Ionian Fuel Refinery";
  return taken
    ? `Needs ${name} · the ${nameOf(plan, taken.holder.unit)} from ${systemLabel(plan.tiles, taken.holder.origin)} has it`
    : `Needs ${name} · already used`;
}

/** Why no ship loads units that use capacity. */
const UNLOADABLE: Record<Unloadable, string> = {
  command_token: "Your command token is here · no pickup",
  no_capacity: "No ship with capacity starts or passes here",
  not_offered: "No ship can load them",
};

/** "2 Infantry · Planet Jord": units that no ship loads, and where they are. */
const unloadableText = (plan: Plan, cargo: UnloadableCargo) =>
  `${cargo.count} ${nameOf(plan, cargo.unit, cargo.damaged, cargo.count)} · ${placeOf(plan, cargo)}`;

/**
 * "Stay: 2 Infantry (Planet Jord), 1 Mech (Planet Jord) · No ship with capacity starts or passes
 * here": the units of a system that no ship loads, by reason. Null when there are none.
 */
function stayText(plan: Plan, system: string): string | null {
  const reasons = new Map<Unloadable, string[]>();
  for (const cargo of plan.facts.unloadable ?? []) {
    if (cargo.system === system) {
      const units = `${cargo.count} ${nameOf(plan, cargo.unit, cargo.damaged, cargo.count)} (${placeOf(plan, cargo)})`;
      reasons.set(cargo.reason, [...(reasons.get(cargo.reason) ?? []), units]);
    }
  }
  return (
    [...reasons]
      .map(([reason, units]) => `Stay: ${units.join(", ")} · ${UNLOADABLE[reason]}`)
      .join(" · ") || null
  );
}

/** Where the units of a pool are in their system: "Planet Jord", "Space area". */
function placeOf(plan: Plan, pool: Pick<CargoPool, "system" | "source">) {
  const planet = plan.tiles
    .get(pool.system)
    ?.planets?.find((item) => item.id === pool.source)?.label;
  return pool.source ? `Planet ${planet ?? pool.source}` : "Space area";
}

function holdOf(plan: Plan, ship: ShipFact): ShipHoldView | null {
  const id = shipId(ship);
  const hold = plan.draft[id];
  if (!hold || ship.capacity <= 0) {
    return null;
  }
  const loaded = holdLoad(plan.draft, id);
  return {
    capacity: ship.capacity,
    loaded,
    slots: Object.entries(hold).flatMap(([pool, count]) => {
      const from = plan.facts.cargo[Number(pool)];
      return Array.from({ length: count }, () => ({
        key: loadKey(id, Number(pool)),
        unit: typeOf(from.unit),
        name: nameOf(plan, from.unit, from.damaged),
        value: count - 1,
        site: from.system === ship.origin ? null : systemLabel(plan.tiles, from.system),
        place: placeOf(plan, from),
      }));
    }),
    accepts:
      loaded < ship.capacity
        ? Object.fromEntries(
            (ship.loads ?? [])
              .filter((pool) => poolLeft(plan.facts, plan.draft, pool) > 0)
              .map((pool) => [
                sourceId(pool),
                { key: loadKey(id, pool), value: (hold[pool] ?? 0) + 1 },
              ]),
          )
        : {},
  };
}

/** The move value of a ship before a boost. A nebula caps it: the game says so, not this text. */
const moveText = (ship: ShipFact) =>
  `Move ${ship.move_value}${ship.nebula && ship.move_value !== 1 ? " → 1 · Nebula" : ""}`;

/** The move value of a ship that moves with Gravity Drive or the Ionian Fuel Refinery: +1 each. */
function boostedText(ship: ShipFact): string | null {
  const boosts = Number(!!ship.move?.gravity_drive) + Number(!!ship.move?.ionian);
  return boosts && !ship.nebula ? `Move ${ship.move_value} → ${ship.move_value + boosts}` : null;
}

/** The ships of one system that are the same kind: one heading, one token for each. */
const kindKey = (ship: ShipFact) => `${ship.origin}:${ship.unit}${ship.damaged ? ":damaged" : ""}`;
/** The unit id of a row of ships, from its key. */
const unitOfRow = (row: MoveShipView) => row.key.split(":")[1];
const routeId = (ship: ShipFact) => ship.move!.path.join(">");

function shipRows(plan: Plan, ships: ShipFact[], onlyMoving = false): MoveShipView[] {
  const kinds = new Map<string, ShipFact[]>();
  for (const ship of ships) {
    kinds.set(kindKey(ship), [...(kinds.get(kindKey(ship)) ?? []), ship]);
  }
  return [...kinds.entries()].map(([key, same]): MoveShipView => {
    const first = same[0];
    const name = nameOf(plan, first.unit, first.damaged);
    // One move value for every ship of the kind is said once, in the heading.
    const shared = same.every((ship) => moveText(ship) === moveText(first)) ? moveText(first) : "";
    const units = same.map((ship, index): ShipUnitView => {
      const moves = shipId(ship) in plan.draft;
      const reason = moves ? null : reasonOf(plan, ship);
      return {
        key: shipKey(shipId(ship)),
        label: `${name} ${index + 1} of ${same.length}`,
        moves,
        canMove: !reason && !!ship.move,
        reason,
        // A ship is staged only when it can move (`canMove`), so a staged ship is never invalid.
        invalid: false,
        move: (moves && boostedText(ship)) || (shared ? null : moveText(ship)),
        route: ship.move ? viaText(plan, ship) : "",
        riftRoll: !!ship.move?.rifts.length,
        // The game uses Gravity Drive only for a ship that cannot arrive without it.
        boost: moves && ship.move?.gravity_drive ? { on: true, locked: true, reason: null } : null,
        hold: holdOf(plan, ship),
        // The row of a ship lights its own path: with Gravity Drive it is not that of its kind.
        link: ship.move ? [`rt:${routeId(ship)}`, `sys:${ship.origin}`] : [`sys:${ship.origin}`],
      };
    });
    const moving = units.filter((unit) => unit.moves);
    const routes = [...new Set(same.filter((ship) => ship.move).map(routeId))];
    return {
      key,
      unit: typeOf(first.unit),
      name,
      damaged: first.damaged,
      count: moving.length,
      total: same.length,
      facts: [
        [shared, first.capacity > 0 ? `Capacity ${first.capacity}` : ""]
          .filter(Boolean)
          .join(" · "),
      ],
      reason: same.every((ship) => ship.blocked) ? BLOCKED[first.blocked!] : null,
      invalid: false,
      link: [`sys:${first.origin}`, ...routes.map((id) => `rt:${id}`)],
      units: onlyMoving ? moving : units,
    };
  });
}

function cargoOf(plan: Plan, origin: string, ships: ShipFact[]): CargoSourceView[] {
  const pools = [...new Set(ships.flatMap((ship) => (ship.move ? (ship.loads ?? []) : [])))];
  return pools
    .sort((a, b) => a - b)
    .map((index) => {
      const pool = plan.facts.cargo[index];
      return {
        id: sourceId(index),
        unit: typeOf(pool.unit),
        name: nameOf(plan, pool.unit, pool.damaged),
        place: placeOf(plan, pool),
        site:
          pool.system === origin
            ? null
            : { system: pool.system, label: systemLabel(plan.tiles, pool.system) },
        left: poolLeft(plan.facts, plan.draft, index),
        reason: null,
      };
    })
    .sort((a, b) => Number(!!a.site) - Number(!!b.site));
}

function originOf(plan: Plan, origin: string, onlyMoving = false): OriginView {
  const ships = plan.facts.ships.filter((ship) => ship.origin === origin);
  const moving = ships.filter((ship) => shipId(ship) in plan.draft);
  const loaded = moving.reduce((sum, ship) => sum + holdLoad(plan.draft, shipId(ship)), 0);
  const stays = shipRows(
    plan,
    ships.filter((ship) => !moving.includes(ship)),
  ).map((row) => `${row.total} ${nameOf(plan, unitOfRow(row), row.damaged, row.total)}`);
  const first = ships.find((ship) => ship.move);
  return {
    system: origin,
    label: systemLabel(plan.tiles, origin),
    away: first ? first.move!.path.length - 1 : 0,
    ships: shipRows(plan, onlyMoving ? moving : ships, onlyMoving),
    cargo: onlyMoving ? [] : cargoOf(plan, origin, ships),
    leaves: moving.length
      ? `${plural(moving.length, "ship")}${loaded ? ` · ${loaded} cargo` : ""}`
      : "",
    loaded,
    capacity: moving.reduce((sum, ship) => sum + ship.capacity, 0),
    stays: stays.join(" · ") || "Nothing",
    // Units that no ship can load stay; the engine says which and why.
    warning: stayText(plan, origin),
    handled: moving.length > 0 || plan.handled.includes(origin),
  };
}

/** The systems that a ship can leave, in the order of the facts. */
const originsOf = (facts: MovementFacts) => [
  ...new Set(facts.ships.filter((ship) => ship.move).map((ship) => ship.origin)),
];

/** What arrives in the active system: every staged ship and what it carries, by unit. */
function arriving(plan: Plan): Counted {
  return countUnits(
    plan.facts.ships.flatMap((ship): [string, number][] => {
      const hold = plan.draft[shipId(ship)];
      return hold
        ? [
            [ship.unit, 1],
            ...Object.entries(hold).map(([pool, count]): [string, number] => [
              plan.facts.cargo[Number(pool)].unit,
              count,
            ]),
          ]
        : [];
    }),
    plan.printed,
  );
}

/**
 * The units of a seat before and after the staged movement, by the printed unit, in display
 * order: `now` are those there, `more` what arrives.
 */
function fleetRows(now: Counted, more: Counted, printed: Catalog | null) {
  const ids = new Map([...now, ...more].map(([key, { id }]) => [key, id]));
  return [...ids]
    .sort(([, a], [, b]) => byUnitOrder(a, b))
    .map(([key, id]) => {
      const there = now.get(key)?.count ?? 0;
      const after = there + (more.get(key)?.count ?? 0);
      return { id, unit: typeOf(id), name: unitLabel(id, printed, after), now: there, after };
    });
}

/**
 * The odds of the space combat that the staged movement starts. `kept` is the last answer of the
 * engine: of another movement it is shown as pending, with the ships that fight now.
 */
function oddsOf(plan: Plan, kept: KeptOdds | null): SpaceOddsView | null {
  const asked = oddsRequest(plan.facts, plan.draft);
  const battle = plan.facts.battle;
  if (!asked || !battle) {
    return null;
  }
  const pending = kept?.key !== asked.key;
  // The engine had no odds for these sides.
  if (!pending && !kept?.response) {
    return null;
  }
  const response = kept?.response ?? null;
  // The ships that fight are those of this movement; the survivors are of the answer.
  const { attacker, defender } = asked.request;
  return combatOdds({
    attacker: plan.viewer,
    defender: battle.defender,
    response,
    draw: response ? response.mutual_destruction_rate + response.unresolved_rate : 0,
    pending,
    fielded: { attacker: attacker.units, defender: defender.units },
    keep: isShip,
    printed: plan.printed,
  });
}

const stagedShips = (plan: Plan) => plan.facts.ships.filter((ship) => shipId(ship) in plan.draft);
/** Fleet supply in the active system with the staged ships. */
const supplyUsed = (plan: Plan) =>
  plan.facts.fleet_supply.charged + stagedShips(plan).filter((ship) => ship.fleet).length;

export function selectMovement(
  update: SessionUpdate,
  facts: MovementFacts,
  draft: MovementDraft,
  handled: string[] = [],
  /** The last answer of the engine for the odds of a space combat. */
  odds: KeptOdds | null = null,
  /** The public rolls of the action so far: a movement that another decision stopped has some. */
  rolls: readonly PublicRoll[] = [],
  printed: Catalog | null = null,
): MovementView {
  const plan = planOf(update, facts, draft, handled, printed);
  const origins = originsOf(facts);
  const stuck = facts.ships.filter((ship) => ship.blocked && !origins.includes(ship.origin));
  // Units that no ship loads in a system that no ship leaves: an origin says it in its card.
  const left = (facts.unloadable ?? []).filter((cargo) => !origins.includes(cargo.system));
  const there = (update.view.board.systems[facts.active]?.units ?? []).filter(
    (unit) => unit.owner === plan.viewer && unit.planet === null,
  );
  const fleet = fleetRows(
    countUnits(
      there.map((unit) => [unit.unit_type, 1]),
      printed,
    ),
    arriving(plan),
    printed,
  );
  const moving = stagedShips(plan);
  const loaded = moving.reduce((sum, ship) => sum + holdLoad(draft, shipId(ship)), 0);
  const room = moving.reduce((sum, ship) => sum + ship.capacity, 0);
  const used = supplyUsed(plan);
  return {
    kind: "movement",
    editing: true,
    target: { system: facts.active, label: systemLabel(plan.tiles, facts.active) },
    origins: origins.map((origin) => originOf(plan, origin)),
    unreachable:
      stuck.length || left.length
        ? {
            target: facts.active,
            rows: [
              ...shipRows(plan, stuck).map((row) => ({
                unit: row.unit,
                name:
                  row.total > 1
                    ? `${row.total} ${nameOf(plan, unitOfRow(row), row.damaged, row.total)}`
                    : row.name,
                text: `${systemLabel(plan.tiles, row.key.split(":")[0])} · ${row.reason}`,
                system: row.key.split(":")[0],
              })),
              ...left.map((cargo) => ({
                unit: typeOf(cargo.unit),
                name: unloadableText(plan, cargo),
                text: `${systemLabel(plan.tiles, cargo.system)} · ${UNLOADABLE[cargo.reason]}`,
                system: cargo.system,
              })),
            ],
          }
        : null,
    arrival: fleet
      .filter((row) => row.after > row.now)
      .map(({ unit, name, now, after }) => ({ unit, name, now, after })),
    gauges: [
      { label: "Fleet supply", used, total: facts.fleet_supply.limit },
      ...(room ? [{ label: "Transport capacity", used: loaded, total: room }] : []),
    ],
    excessShips: Math.max(0, used - facts.fleet_supply.limit),
    riftRolls: moving.reduce((sum, ship) => sum + (ship.move?.rifts.length ?? 0), 0),
    rift: riftRows(rolls, (id) => systemLabel(plan.tiles, id), printed),
    removed: removedUnits(rolls, printed),
    // The guns before the roll, unless a space combat follows: its odds count them. The ships of
    // the viewer that the guns fire at are those that are there after the movement.
    cannon: cannonOf(
      update,
      facts.active,
      plan.viewer,
      rolls,
      printed,
      facts.battle ? [] : (facts.cannon ?? []),
      (seat) =>
        seat === plan.viewer
          ? fleet
              .filter((row) => isShip(row.unit) && row.after > 0)
              .map((row) => ({ unit: row.unit, name: row.name, count: row.after }))
          : null,
    ),
    odds: oddsOf(plan, odds),
  };
}

/** The ships of each seat in the space area of `active` now, by kind. */
function shipsThere(update: SessionUpdate, active: string, printed: Catalog | null): ShipsOf {
  return (seat) =>
    forceOf(
      (update.view.board.systems[active]?.units ?? []).filter(
        (unit) => unit.owner === seat && unit.planet === null,
      ),
      printed,
    )
      .filter((item) => isShip(item.unit))
      .map((item) => ({ unit: item.unit, name: item.name, count: item.count }));
}

/**
 * The space cannon offense of the movement: what it rolled, else the guns that will roll (the
 * facts, before the movement is sent). `ships` overrides the ships of a seat in the system.
 */
function cannonOf(
  update: SessionUpdate,
  active: string,
  viewer: string,
  rolls: readonly PublicRoll[],
  printed: Catalog | null,
  guns: readonly CannonFact[],
  ships: (seat: string) => ReturnType<ShipsOf> | null = () => null,
): BattleTableView | null {
  const there = shipsThere(update, active, printed);
  const fired = (seat: string) => ships(seat) ?? there(seat);
  return (
    cannonTable(rolls, viewer, viewer, fired, printed) ??
    cannonPreview(guns, viewer, viewer, fired, printed)
  );
}

/**
 * The parts of the movement step, for its heading: the move, a gravity rift when a ship rolls for
 * one, and the space cannon offense. `guns` are the guns of the facts while the movement is open;
 * without them (after the movement) only a roll says that the offense was there. It is skipped
 * only when the facts have no gun.
 */
export function movementTrail(
  rolls: readonly PublicRoll[],
  open: boolean,
  riftRolls: number,
  guns: readonly CannonFact[] | null,
): TrailView {
  const rifts = riftRows(rolls, (id) => id) !== null;
  const fired = cannonTable(rolls, "", "") !== null;
  return [
    { label: "Move ships", status: open ? "active" : "complete" },
    ...(riftRolls > 0
      ? [{ label: "Gravity rift", status: "todo" as const }]
      : rifts
        ? [{ label: "Gravity rift", status: "complete" as const }]
        : []),
    ...(fired
      ? [{ label: "Space cannon offense", status: "complete" as const }]
      : guns?.length
        ? [{ label: "Space cannon offense", status: "todo" as const }]
        : guns
          ? [{ label: "Space cannon offense", status: "skipped" as const, reason: "no guns" }]
          : []),
  ];
}

/**
 * The movement of the action after it was made, read-only: the fleet of the viewer in the active
 * system now, and what the dice of the movement did. What moved from where is not told yet.
 */
export function selectMovementResult(
  update: SessionUpdate,
  active: string,
  rolls: readonly PublicRoll[],
  printed: Catalog | null = null,
): MovementView {
  const tiles = tilesOf(update);
  const viewer = update.viewer.role === "player" ? update.viewer.seat : "";
  const there = countUnits(
    (update.view.board.systems[active]?.units ?? [])
      .filter((unit) => unit.owner === viewer && unit.planet === null)
      .map((unit) => [unit.unit_type, 1]),
    printed,
  );
  return {
    kind: "movement",
    editing: false,
    target: { system: active, label: systemLabel(tiles, active) },
    origins: [],
    unreachable: null,
    arrival: fleetRows(there, new Map(), printed).map(({ unit, name, now, after }) => ({
      unit,
      name,
      now,
      after,
    })),
    gauges: [],
    excessShips: 0,
    riftRolls: 0,
    rift: riftRows(rolls, (id) => systemLabel(tiles, id), printed),
    removed: removedUnits(rolls, printed),
    cannon: cannonOf(update, active, viewer, rolls, printed, []),
    odds: null,
  };
}

/** The fleet of one system with its controls, for the board. Null when nothing can leave it. */
export const selectOrigin = (
  update: SessionUpdate,
  facts: MovementFacts,
  draft: MovementDraft,
  system: string | null,
  handled: string[] = [],
  printed: Catalog | null = null,
): OriginView | null =>
  system !== null && originsOf(facts).includes(system)
    ? originOf(planOf(update, facts, draft, handled, printed), system)
    : null;

/** What is committed to the active system: the ships that move, by the system that they leave. */
export function selectArrival(
  update: SessionUpdate,
  facts: MovementFacts,
  draft: MovementDraft,
  odds: KeptOdds | null = null,
  printed: Catalog | null = null,
) {
  const plan = planOf(update, facts, draft, [], printed);
  const origins = originsOf(facts)
    .map((origin) => originOf(plan, origin, true))
    .filter((origin) => origin.ships.length);
  const ships = stagedShips(plan).length;
  const cargo = origins.reduce((sum, origin) => sum + origin.loaded, 0);
  return {
    summary: ships
      ? `${plural(ships, "ship")}${cargo ? ` · ${cargo} cargo` : ""} · fleet supply ${supplyUsed(plan)} / ${facts.fleet_supply.limit}`
      : "Nothing moves here yet",
    origins,
    odds: oddsOf(plan, odds),
  };
}

/**
 * The staged movement on the board: the paths of the ships of the open system, and for each
 * system what the movement changes there. The game chooses a path; the other paths are not drawn.
 */
export function selectMovementBoard(
  update: SessionUpdate,
  facts: MovementFacts,
  draft: MovementDraft,
  inspected: string | null,
  handled: string[] = [],
  printed: Catalog | null = null,
) {
  const plan = planOf(update, facts, draft, handled, printed);
  const groups = new Map<string, ShipFact[]>();
  for (const ship of facts.ships) {
    if (ship.origin === inspected && ship.move) {
      groups.set(routeId(ship), [...(groups.get(routeId(ship)) ?? []), ship]);
    }
  }
  const routes = [...groups.entries()].map(([id, ships]): RouteView => {
    const moving = ships.filter((ship) => shipId(ship) in draft);
    const move = ships[0].move!;
    return {
      id,
      path: move.path,
      staged: moving.length > 0,
      marks: moving.length
        ? [
            ...move.rifts.map((system) => ({ system, sign: "die" as const, text: RIFT_RULE })),
            ...(moving.some((ship) => ship.move!.gravity_drive)
              ? [{ system: move.path[0], sign: "plus" as const, text: "Gravity Drive: +1 move" }]
              : []),
          ]
        : [],
    };
  });
  const origins = originsOf(facts);
  const notes: Record<string, NonNullable<TileView["note"]>> = {};
  for (const ship of facts.ships) {
    if (ship.blocked && !origins.includes(ship.origin)) {
      notes[ship.origin] = { sign: "stay", text: `Your ships stay · ${BLOCKED[ship.blocked]}` };
    }
  }
  for (const cargo of facts.unloadable ?? []) {
    if (!origins.includes(cargo.system)) {
      const text = `${unloadableText(plan, cargo)} cannot be loaded · ${UNLOADABLE[cargo.reason]}`;
      const note = notes[cargo.system];
      notes[cargo.system] =
        note?.sign === "cargo"
          ? { ...note, text: `${note.text}\n${text}` }
          : (note ?? { sign: "cargo", text });
    }
  }
  /** For each system: the ships and the ground forces that the staged movement takes from it. */
  const leaving: Record<
    string,
    { ships: number; planets: Record<string, number>; picked: number }
  > = {};
  const at = (system: string) => (leaving[system] ??= { ships: 0, planets: {}, picked: 0 });
  for (const ship of stagedShips(plan)) {
    at(ship.origin).ships += 1;
    for (const [pool, count] of Object.entries(draft[shipId(ship)])) {
      const from = facts.cargo[Number(pool)];
      const system = at(from.system);
      if (from.source) {
        system.planets[from.source] = (system.planets[from.source] ?? 0) + count;
      } else if (isShip(typeOf(from.unit))) {
        system.ships += count;
      }
      system.picked += from.system === ship.origin ? 0 : count;
    }
  }
  const arrives = [...arriving(plan).values()]
    .filter(({ id }) => isShip(typeOf(id)))
    .reduce((sum, { count }) => sum + count, 0);
  return {
    routes,
    notes,
    leaving,
    arrives,
    origins,
    handled: origins.filter((origin) => handled.includes(origin)),
    active: facts.active,
    viewer: plan.viewer,
    // The systems that the movement is about, for "Fit task".
    systems: [...new Set([facts.active, ...origins, ...routes.flatMap((route) => route.path)])],
  };
}

export type MovementBoard = ReturnType<typeof selectMovementBoard>;

/** A tile with what the staged movement changes there: "now → after", and the mark of a change. */
export function withMovement(tile: TileView, move: MovementBoard): TileView {
  const here = tile.id === move.active;
  // Units that are loaded in the active system stay in it.
  const gone = here ? undefined : move.leaving[tile.id];
  const change = here ? move.arrives : -(gone?.ships ?? 0);
  const fleets = tile.fleets.some((fleet) => fleet.seat === move.viewer)
    ? tile.fleets
    : [...tile.fleets, { seat: move.viewer, ships: 0, strength: 0 }];
  const marks = fleets
    .map((fleet) =>
      fleet.seat === move.viewer && change
        ? { ...fleet, ships: fleet.ships + change, was: fleet.ships }
        : fleet,
    )
    .filter((fleet) => fleet.ships || fleet.was);
  const picked = here ? 0 : (gone?.picked ?? 0);
  return {
    ...tile,
    planets: tile.planets.map((planet) => ({
      ...planet,
      groundForces: planet.groundForces - (gone?.planets[planet.id] ?? 0),
    })),
    fleets: marks,
    control: marks.length > 1 ? "contested" : (marks[0]?.seat ?? null),
    pickedUp: picked,
    note: move.notes[tile.id],
    handled:
      (!!gone && (gone.ships > 0 || picked > 0 || Object.keys(gone.planets).length > 0)) ||
      move.handled.includes(tile.id),
  };
}
