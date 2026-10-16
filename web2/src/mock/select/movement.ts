// The movement step: what can leave each system, what the ships load, and what arrives. The same
// plan gives the board its routes and its marks, so the map shows every staged decision.
import type {
  CargoSourceView,
  MoveShipView,
  MovementView,
  OriginView,
  Rich,
  RouteView,
  ShipHoldView,
  ShipUnitView,
  TileView,
  UnitType,
} from "../../model";
import { plural } from "../../model";
import { MAP, SPECS, SUPPLY, U, lineById, LINES, type Line } from "../data";
import { E } from "../loose";
import { sysLabel, type State } from "../world";
import { battleTable } from "./battle";

const RIFT_RULE = "Each ship that leaves a gravity rift rolls: destroyed on 1–3, with its cargo";
const lineName = (line: Line, count = 1) =>
  (line.label || E.unitName(line.type, count)) + (line.damaged ? " (damaged)" : "");
/** In a sentence: "the carrier", and "the Cruiser II" for a unit with a name of its own. */
const inText = (line: Line) => line.label || lineName(line).toLowerCase();
const place = (line: Line) => (line.from === "space" ? "Space area" : `Planet ${line.from}`);
const isShip = (line: Line) => !!U[line.type].ship;

/** "via #23 Tar’Mann · α wormhole": the systems between the start and the active system. */
function viaText(path: string[]) {
  const inner = path.slice(1, -1);
  const hop = path.find((id, index) => index && !E.adjacent(path[index - 1]).includes(id));
  return `${inner.length ? `via ${inner.map((id) => `#${id} ${MAP[id].name}`).join(", ")}` : "Direct"}${hop ? ` · ${MAP[hop].wormhole} wormhole` : ""}`;
}
/** "1 Cruiser · 2 Fighters", from counts by line. */
const countsText = (counts: [Line, number][]) =>
  counts
    .filter(([, count]) => count > 0)
    .map(([line, count]) => `${count} ${lineName(line, count)}`)
    .join(" · ");

function plan(state: State) {
  const editing = state.edit?.step === 1;
  const data: Record<string, number> = E.editedData(state, 1);
  const lines = LINES;
  const active: string = E.activeId(state);
  const info = (line: Line) => E.moveInfo(state, line);
  const keys = (line: Line): string[] => E.shipKeys(line);
  const path = (unit: string): string[] | undefined => E.shipPath(state, data, unit);
  /** How many ships of a line move. */
  const staged = (line: Line) => keys(line).filter((unit) => data[unit]).length;
  const loadedFrom = (cargo: Line) =>
    (E.stagedEntries(data) as any[])
      .filter((entry) => entry.line === cargo && entry.ship)
      .reduce((sum, entry) => sum + entry.count, 0);
  /** "the carrier from Lodor": the ship that has Gravity Drive. */
  const holder = (unit: string) => {
    const line: Line = E.shipLine(unit);
    return `the ${inText(line)} from ${MAP[line.origin].name}`;
  };

  const holdOf = (unit: string): ShipHoldView | null => {
    const ship: Line = E.shipLine(unit);
    if (!U[ship.type].capacity || !data[unit]) {
      return null;
    }
    const entries: any[] = E.holdEntries(data, unit);
    const loaded: number = E.holdLoad(data, unit);
    const capacity: number = E.holdRoom(data, unit);
    return {
      capacity,
      loaded,
      slots: entries.flatMap((entry) =>
        Array.from({ length: entry.count }, () => ({
          key: entry.key,
          unit: entry.line.type as UnitType,
          name: E.unitName(entry.line.type) as string,
          value: entry.count - 1,
          site: entry.line.origin === ship.origin ? null : sysLabel(entry.line.origin),
          place: place(entry.line),
        })),
      ),
      accepts:
        editing && loaded < capacity
          ? Object.fromEntries(
              (E.loadable(state, data, unit) as any[])
                .filter(
                  (item) =>
                    !item.token && E.available(state, data, item.key) > (data[item.key] || 0),
                )
                .map((item) => [item.line.id, { key: item.key, value: (data[item.key] || 0) + 1 }]),
            )
          : {},
    };
  };

  const unitRow = (line: Line, unit: string, index: number): ShipUnitView => {
    const move = info(line);
    const moves = !!data[unit];
    const route = move.routes.length ? path(unit) : undefined;
    const other: string | undefined = E.boosted(state, data, unit);
    const on: boolean = E.hasBoost(data, unit);
    const blocked = move.gd && !!other;
    const reason = !move.routes.length
      ? (move.reason as string)
      : blocked
        ? `Needs Gravity Drive · ${holder(other!)} has it`
        : null;
    return {
      key: unit,
      label: `${lineName(line)} ${index + 1} of ${line.n}`,
      moves,
      canMove: move.routes.length > 0 && !blocked,
      reason,
      invalid: moves && (!move.routes.length || blocked),
      move: moves && on && !move.capped ? `Move ${move.base} → ${move.base + 1}` : null,
      route: route ? viaText(route) : "",
      riftRoll: !!route && E.rifts(route).length > 0,
      boost:
        moves && (move.gd || move.helps)
          ? {
              on,
              locked: move.gd,
              reason:
                !on && other ? `Gravity Drive moves one ship · ${holder(other)} has it` : null,
            }
          : null,
      hold: holdOf(unit),
      link: [`rt:${line.id}`, `sys:${line.origin}`],
    };
  };

  const shipRow = (line: Line): MoveShipView => {
    const move = info(line);
    const count = staged(line);
    const units = keys(line)
      .map((unit, index) => unitRow(line, unit, index))
      .filter((row) => editing || row.moves);
    const facts: Rich = [
      `Move ${move.base}${move.capped ? " → 1 · Nebula" : ""}`,
      U[line.type].capacity ? ` · Capacity ${U[line.type].capacity}` : "",
    ];
    return {
      key: line.id,
      unit: line.type,
      name: lineName(line, editing ? 1 : count),
      damaged: !!line.damaged,
      count,
      total: line.n,
      facts,
      reason: move.routes.length ? null : (move.reason as string),
      invalid: units.some((row) => row.invalid),
      link: [`rt:${line.id}`, `sys:${line.origin}`],
      units,
    };
  };

  /** What the ships of a system can load: the units there, then those on the way of a ship. */
  const sources = (origin: string): CargoSourceView[] => {
    const seen = new Map<string, CargoSourceView>();
    for (const line of lines.filter((ship) => ship.origin === origin && isShip(ship))) {
      for (const unit of info(line).routes.length ? keys(line) : []) {
        for (const item of E.loadable(state, data, unit) as any[]) {
          const cargo: Line = item.line;
          seen.set(cargo.id, {
            id: cargo.id,
            unit: cargo.type,
            name: E.unitName(cargo.type, 1),
            place: place(cargo),
            site: item.pickup ? { system: cargo.origin, label: sysLabel(cargo.origin) } : null,
            left: Math.max(0, cargo.n - loadedFrom(cargo)),
            reason: item.token ? "Your command token is here · no pickup" : null,
          });
        }
      }
    }
    return [...seen.values()].sort((a, b) => (a.site ? 1 : 0) - (b.site ? 1 : 0));
  };

  const shipsAt = (origin: string) =>
    lines.filter((line) => line.origin === origin && isShip(line));
  const reaches = (origin: string) => shipsAt(origin).some((line) => info(line).routes.length);
  const used = (origin: string) => shipsAt(origin).some((line) => staged(line));
  const away = (origin: string) =>
    Math.min(
      ...shipsAt(origin)
        .filter((line) => info(line).routes.length)
        .map((line) => info(line).routes[0].length - 1),
      9,
    );
  const origins = [...new Set(lines.filter(isShip).map((line) => line.origin))].filter(
    (origin) => origin !== active,
  );
  const inRange = origins
    .filter((origin) => (editing ? reaches(origin) || used(origin) : used(origin)))
    .sort((a, b) => away(a) - away(b));

  /** The player marked the system as done, in this movement. */
  const marked = (origin: string) => state.handled?.[origin] === active;

  const card = (origin: string): OriginView => {
    const ships = shipsAt(origin)
      .map(shipRow)
      .filter((row) => editing || row.count > 0);
    const moving = ships.reduce((sum, row) => sum + row.count, 0);
    const holds = ships.flatMap((row) => row.units).flatMap((row) => row.hold ?? []);
    const loaded = holds.reduce((sum, hold) => sum + hold.loaded, 0);
    const cargo = editing ? sources(origin) : [];
    const here = lines.filter((line) => line.origin === origin);
    const left = here.map((line): [Line, number] => [
      line,
      line.n - (isShip(line) ? staged(line) : loadedFrom(line)),
    ]);
    const room = left
      .filter(([line]) => isShip(line))
      .reduce((sum, [line, count]) => sum + count * (U[line.type].capacity || 0), 0);
    const fighters = left
      .filter(([line]) => line.type === "fighter")
      .reduce((sum, [, count]) => sum + count, 0);
    return {
      system: origin,
      label: sysLabel(origin),
      away: away(origin),
      ships,
      cargo,
      leaves: moving ? `${plural(moving, "ship")}${loaded ? ` · ${loaded} cargo` : ""}` : "",
      loaded,
      capacity: holds.reduce((sum, hold) => sum + hold.capacity, 0),
      stays: countsText(left) || "Nothing",
      warning:
        moving && !MAP[origin].dock && fighters > room
          ? `${plural(fighters - room, "fighter")} with no capacity here · removed`
          : null,
      handled: moving > 0 || marked(origin),
    };
  };

  // What cannot move: the ships of a system where no ship reaches, and the cargo that no ship
  // with capacity can load.
  const carriers = lines.filter(
    (line) => isShip(line) && U[line.type].capacity && info(line).routes.length,
  );
  const stuckCargo = lines
    .filter((line) => !isShip(line) && line.origin !== active)
    .filter(
      (cargo) =>
        !carriers.some((ship) =>
          keys(ship).some((unit) =>
            (E.loadable(state, data, unit) as any[]).some(
              (item) => item.line === cargo && !item.token,
            ),
          ),
        ),
    )
    .map((cargo) => {
      const passing = lines.filter(
        (line) =>
          isShip(line) &&
          info(line).routes.length &&
          keys(line).some((unit) => path(unit)!.includes(cargo.origin)),
      );
      const reason = MAP[cargo.origin].token
        ? "Your command token is here"
        : passing.length
          ? `No capacity on the ${passing.map(inText).join(", ")} that ${passing.some((line) => line.origin === cargo.origin) ? "starts" : "passes"} here`
          : "No ship with capacity passes here";
      return { cargo, reason };
    });
  const stuckShips = origins
    .filter((origin) => !inRange.includes(origin))
    .flatMap(shipsAt)
    .map((line) => ({ line, reason: info(line).reason as string }));

  // What arrives, by unit type, on top of what the player has in the active system.
  const present: Record<string, number> = state.present || {};
  const arriving: Record<string, number> = {};
  for (const { line, count } of E.stagedEntries(data) as any[]) {
    arriving[line.type] = (arriving[line.type] || 0) + count;
  }
  const types = (Object.keys(U) as UnitType[]).filter((type) => present[type] || arriving[type]);
  const after = (type: string) => (present[type] || 0) + (arriving[type] || 0);
  const shipsAfter = types
    .filter((type) => U[type].ship)
    .reduce((sum, type) => sum + after(type), 0);
  const cargoAfter = types
    .filter((type) => !U[type].ship)
    .reduce((sum, type) => sum + after(type), 0);
  const roomAfter = types.reduce((sum, type) => sum + after(type) * (U[type].capacity || 0), 0);

  return {
    editing,
    data,
    lines,
    active,
    info,
    keys,
    path,
    staged,
    origins: inRange.map(card),
    stuckShips,
    stuckCargo,
    arrival: types.map((unit) => ({ unit, now: present[unit] || 0, after: after(unit) })),
    shipsNow: types
      .filter((type) => U[type].ship)
      .reduce((sum, type) => sum + (present[type] || 0), 0),
    shipsAfter,
    cargoAfter,
    roomAfter,
  };
}

export function movement(state: State): MovementView {
  const it = plan(state);
  const { editing } = it;
  const exits: any[] = E.riftExits(state, it.data);
  const riftName = (ship: any) => lineById[ship.line].label || E.unitName(ship.type);
  /** " · carries 2 Infantry, 1 Fighter": what is destroyed with the ship. */
  const carries = (ship: any) => {
    const types = Object.keys(ship.cargo ?? {}) as UnitType[];
    return types.length
      ? ` · carries ${types.map((type) => `${ship.cargo[type]} ${E.unitName(type, ship.cargo[type])}`).join(", ")}`
      : "";
  };
  let rift: MovementView["rift"] = null;
  if (!editing && state.rift) {
    rift = state.rift.map((ship: any) => ({
      unit: ship.type,
      name: riftName(ship),
      text: `From ${MAP[lineById[ship.line].origin].name} · left gravity rift #${ship.rift}${carries(ship)} · survives on 4+`,
      roll: { face: ship.face, lost: ship.lost },
    }));
  } else if (!editing && state.boundary === "rift") {
    rift = exits.map((ship) => ({
      unit: ship.type,
      name: riftName(ship),
      text: `From ${MAP[lineById[ship.line].origin].name} · leaves gravity rift #${ship.rift}${carries(ship)}`,
      roll: null,
    }));
  }
  let cannon: MovementView["cannon"] = null;
  if (!editing && state.cannon) {
    cannon = battleTable(state, state.cannon);
  } else if (!editing && state.boundary === "cannon") {
    cannon = battleTable(
      state,
      E.preview(
        "Space cannon offense",
        { att: state.fleet, def: E.force({ pds: 1 }) },
        { def: SPECS.cannon },
      ),
    );
  }
  const stuck = [
    ...it.stuckShips.map(({ line, reason }) => ({
      unit: line.type,
      name: lineName(line),
      text: `${sysLabel(line.origin)} · ${reason}`,
      system: line.origin,
    })),
    ...it.stuckCargo.map(({ cargo, reason }) => ({
      unit: cargo.type,
      name: `${cargo.n} ${E.unitName(cargo.type, cargo.n)}`,
      text: `${sysLabel(cargo.origin)} · ${reason}`,
      system: cargo.origin,
    })),
  ];
  return {
    kind: "movement",
    editing,
    target: { system: it.active, label: sysLabel(it.active) },
    origins: it.origins,
    unreachable: editing && stuck.length ? { target: it.active, rows: stuck } : null,
    arrival: it.arrival,
    gauges: [
      { label: "Fleet supply", used: it.shipsAfter, total: SUPPLY },
      { label: "Transport capacity", used: it.cargoAfter, total: it.roomAfter },
    ],
    excessShips: Math.max(0, it.shipsAfter - SUPPLY),
    riftRolls: editing ? exits.length : 0,
    rift,
    removed: editing ? [] : state.removed.map((type: string) => E.unitName(type, 1) as string),
    cannon,
    odds: null,
  };
}

/** The fleet of one system with its controls, for the board. Null when nothing can leave it. */
export const movementOrigin = (state: State, system: string | null) =>
  plan(state).origins.find((origin) => origin.system === system) ?? null;

/** What is committed to the active system: the ships that move, by the system that they leave. */
export function movementArrival(state: State) {
  const it = plan(state);
  const origins = it.origins
    .map((origin) => ({
      ...origin,
      cargo: [],
      ships: origin.ships
        .filter((ship) => ship.count > 0)
        .map((ship) => ({ ...ship, units: ship.units.filter((unit) => unit.moves) })),
    }))
    .filter((origin) => origin.ships.length);
  const ships = origins.reduce(
    (sum, origin) => sum + origin.ships.reduce((count, ship) => count + ship.count, 0),
    0,
  );
  const cargo = origins.reduce((sum, origin) => sum + origin.loaded, 0);
  return {
    summary: ships
      ? `${plural(ships, "ship")}${cargo ? ` · ${cargo} cargo` : ""} · fleet supply ${it.shipsAfter} / ${SUPPLY}`
      : "Nothing moves here yet",
    origins,
    // The demo has its own odds, in the battle table.
    odds: null,
  };
}

/**
 * The staged movement on the board: for each system what the movement changes there, and the
 * paths of the ships of the system that is open. The game chooses a path; the other paths are
 * not drawn, so the map stays quiet.
 */
export function movementBoard(state: State) {
  const it = plan(state);
  // One path for the ships that take the same way. A ship with Gravity Drive has its own.
  const groups = new Map<string, { path: string[]; lines: Line[]; moving: string[] }>();
  for (const line of it.lines) {
    if (line.origin !== state.inspect || !isShip(line) || !it.info(line).routes.length) {
      continue;
    }
    for (const unit of it.keys(line)) {
      const path = it.path(unit)!;
      const key = path.join(">");
      const group = groups.get(key) ?? { path, lines: [], moving: [] };
      groups.set(key, {
        path,
        lines: group.lines.includes(line) ? group.lines : [...group.lines, line],
        moving: it.data[unit] ? [...group.moving, unit] : group.moving,
      });
    }
  }
  const routes = [...groups.entries()].map(([id, group]): RouteView => {
    const ships = group.moving.length;
    const boost = group.moving.some((unit) => E.hasBoost(it.data, unit));
    return {
      id,
      links: group.lines.map((line) => `rt:${line.id}`),
      path: group.path,
      staged: ships > 0,
      marks: ships
        ? [
            ...(E.rifts(group.path) as string[]).map((system) => ({
              system,
              sign: "die" as const,
              text: RIFT_RULE,
            })),
            ...(boost
              ? [{ system: group.path[0], sign: "plus" as const, text: "Gravity Drive: +1 move" }]
              : []),
          ]
        : [],
    };
  });
  const notes: Record<string, NonNullable<TileView["note"]>> = {};
  for (const { line, reason } of it.stuckShips) {
    notes[line.origin] = { sign: "stay", text: `Your ships stay · ${reason}` };
  }
  for (const { cargo, reason } of it.stuckCargo) {
    notes[cargo.origin] ??= {
      sign: "cargo",
      text: `${cargo.n} ${E.unitName(cargo.type, cargo.n)} cannot be loaded · ${reason}`,
    };
  }
  const leaving: Record<string, number> = {};
  for (const line of it.lines) {
    if (isShip(line) && it.staged(line)) {
      leaving[line.origin] = (leaving[line.origin] || 0) + it.staged(line);
    }
  }
  return {
    routes,
    notes,
    leaving,
    // The systems that the player can open: something can leave them.
    origins: it.origins.map((origin) => origin.system),
    handled: it.origins.filter((origin) => origin.handled).map((origin) => origin.system),
    // The systems that the movement is about: they stay in full view while a system is chosen.
    systems: [
      ...new Set([it.active, ...routes.flatMap((route) => route.path), ...Object.keys(notes)]),
    ],
    active: it.active,
    arrives: [it.shipsNow, it.shipsAfter] as [number, number],
  };
}
