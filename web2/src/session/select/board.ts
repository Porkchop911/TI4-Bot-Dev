// The board: one tile for each system on the map, and the inspector of one system.
import type {
  Anomaly,
  BoardTaskView,
  BoardView,
  InspectorRowView,
  InspectorView,
  SeatId,
  TileView,
} from "../../model";
import type { LandingDraft } from "../landingDraft";
import type { MovementDraft } from "../movementDraft";
import type { KeptOdds } from "../movementOdds";
import type { ProductionDraft } from "../productionDraft";
import type {
  BoardTileView,
  ExplorationCatalog,
  InvasionFacts,
  MovementFacts,
  PlacedUnitView,
  ProductionFacts,
  SessionUpdate,
  SystemView,
} from "../wire";
import { planetTraits, printedTraits } from "./exploration";
import { landingTask, selectLanding, withLandings } from "./invasion";
import { selectArrival, selectMovementBoard, selectOrigin, withMovement } from "./movement";
import {
  type Stage,
  paymentSystems,
  productionTask,
  selectBuild,
  withProduction,
} from "./production";
import {
  ANOMALIES,
  TECH_COLORS,
  WORMHOLES,
  forceOf as force,
  isGroundForce,
  isShip,
  unitType,
} from "./names";

const viewerSeat = (update: SessionUpdate) =>
  update.viewer.role === "player" ? update.viewer.seat : null;

const seatsOf = (units: PlacedUnitView[]): SeatId[] => [...new Set(units.map((u) => u.owner))];
const shipsIn = (system: SystemView | undefined) =>
  (system?.units ?? []).filter((unit) => {
    const type = unitType(unit.unit_type);
    return unit.planet === null && type !== null && isShip(type);
  });
const onPlanet = (system: SystemView | undefined, planet: string) =>
  (system?.units ?? []).filter((unit) => unit.planet === planet);

/** Systems outside the map (the wormhole nexus, the fracture) have no place on this board yet. */
const onMap = (tile: BoardTileView) => !tile.special_area;

type Traits = ReturnType<typeof planetTraits>;

function tileView(
  tile: BoardTileView,
  system: SystemView | undefined,
  viewer: string | null,
  traits: Traits,
) {
  const ships = shipsIn(system);
  const fleets = seatsOf(ships).map((seat) => ({
    seat,
    ships: ships.filter((unit) => unit.owner === seat).length,
    // Hits per round is a rule. It comes from the engine later.
    strength: 0,
  }));
  const anomaly = (tile.anomalies ?? [])
    .map((name) => ANOMALIES[name as keyof typeof ANOMALIES] as Anomaly | undefined)
    .find(Boolean);
  const wormhole = tile.wormholes?.[0];
  const view: TileView = {
    id: tile.system_id,
    name: tile.label,
    q: tile.q,
    r: tile.r,
    // The view does not say whose home a system is.
    home: null,
    anomaly: anomaly ?? null,
    wormhole: wormhole ? (WORMHOLES[wormhole] ?? wormhole) : null,
    commandToken: viewer !== null && !!system?.command_tokens.includes(viewer),
    planets: (tile.planets ?? []).slice(0, 3).map((planet) => {
      const units = onPlanet(system, planet.id);
      const types = units.map((unit) => unitType(unit.unit_type));
      return {
        id: planet.id,
        name: planet.label,
        resources: planet.resources,
        influence: planet.influence,
        owner: system?.planets[planet.id]?.controlled_by ?? null,
        tech: TECH_COLORS[planet.tech_specialties?.[0] ?? ""] ?? null,
        groundForces: types.filter((type) => type && isGroundForce(type)).length,
        planetaryShield: types.includes("pds"),
        traits: traits(planet),
        exhausted: !!system?.planets[planet.id]?.exhausted,
      };
    }),
    control: fleets.length > 1 ? "contested" : (fleets[0]?.seat ?? null),
    fleets,
    pickedUp: 0,
  };
  return view;
}

function inspector(
  tile: BoardTileView,
  system: SystemView | undefined,
  viewer: string | null,
  traits: Traits,
): InspectorView {
  const ships = shipsIn(system);
  const space: InspectorRowView[] = seatsOf(ships).map((seat) => ({
    seat,
    force: force(ships.filter((unit) => unit.owner === seat)),
  }));
  const planets = (tile.planets ?? []).map((planet): InspectorRowView => {
    const units = onPlanet(system, planet.id);
    return {
      planet: {
        name: planet.label,
        resources: planet.resources,
        influence: planet.influence,
        owner: system?.planets[planet.id]?.controlled_by ?? null,
        traits: traits(planet),
      },
      force: force(units),
      notes: system?.planets[planet.id]?.exhausted ? [{ text: "Exhausted", tone: "muted" }] : [],
    };
  });
  const tokens = system?.command_tokens ?? [];
  const facts = [
    ...(tile.anomalies ?? []).map((name) => name.charAt(0).toUpperCase() + name.slice(1)),
    ...(tile.wormholes ?? []).map((name) => `${WORMHOLES[name] ?? name} wormhole`),
    tile.hyperlane ? "Hyperlane" : "",
    viewer !== null && tokens.includes(viewer) ? "Your command token is here" : "",
  ].filter(Boolean);
  return {
    system: tile.system_id,
    title: `${tile.label} · ${tile.system_id}`,
    facts: facts.join(" · ") || "No anomaly",
    notes: [],
    rows: [...(space.length ? space : [{ label: "In space", text: "No ships" }]), ...planets],
    activate: null,
  };
}

/** The seat of a home place on a board that has no players yet. */
export const SEAT_PLACE = "place";

/**
 * The board of a table before its game: the tiles only. A home is the place of a seat: nobody
 * sits there yet, so it has no planets and no faction.
 */
export function selectPreviewBoard(
  tiles: BoardTileView[],
  homes: string[],
  inspected: string | null,
): BoardView {
  const map = tiles.filter(onMap);
  const place = (tile: BoardTileView): BoardTileView =>
    homes.includes(tile.system_id) ? { ...tile, label: "Home of a seat", planets: [] } : tile;
  const open = map.find((tile) => tile.system_id === inspected);
  const marks = map.flatMap((tile) =>
    (tile.wormholes ?? []).map((kind) => [kind, tile.system_id] as const),
  );
  return {
    tiles: map.map((tile) => ({
      ...tileView(place(tile), undefined, null, printedTraits),
      home: homes.includes(tile.system_id) ? SEAT_PLACE : null,
    })),
    wormholes: marks.flatMap(([kind, id]) =>
      marks
        .filter(([other, otherId]) => other === kind && otherId > id)
        .map(([, otherId]): [string, string] => [id, otherId]),
    ),
    routes: [],
    activeSystem: null,
    inspected: open ? open.system_id : null,
    targeting: false,
    task: null,
    inspector: open ? inspector(place(open), undefined, null, printedTraits) : null,
    origin: null,
    landing: null,
    production: null,
    taskSystems: [],
    // The board is framed again when the map is another one.
    fitKey: `preview:${map.map((tile) => tile.system_id).join(",")}`,
  };
}

export function selectBoard(
  update: SessionUpdate,
  inspected: string | null,
  task: BoardTaskView | null,
  /** The open movement step: the board shows what is staged, and a system opens its fleet. */
  movement: {
    facts: MovementFacts;
    draft: MovementDraft;
    handled: string[];
    /** The last answer of the engine for the odds of the space combat there. */
    odds?: KeptOdds | null;
  } | null = null,
  /** The open landings: the planets of the active system are the choice, and it opens them. */
  landing: { facts: InvasionFacts; draft: LandingDraft; chosen: number | null } | null = null,
  /** The printed names and texts of the attachments. */
  catalog: ExplorationCatalog | null = null,
  /** The open production: the active system opens the build, then the planets pay. */
  production: {
    facts: ProductionFacts;
    draft: ProductionDraft;
    stage: Stage;
  } | null = null,
): BoardView {
  const traits = planetTraits(update, catalog);
  const viewer = viewerSeat(update);
  const board = update.view.board;
  const map = (board.map_tiles ?? []).filter(onMap);
  const open = map.find((tile) => tile.system_id === inspected);
  const marks = map.flatMap((tile) =>
    (tile.wormholes ?? []).map((kind) => [kind, tile.system_id] as const),
  );
  const move =
    movement &&
    selectMovementBoard(
      update,
      movement.facts,
      movement.draft,
      open?.system_id ?? null,
      movement.handled,
    );
  // A sheet of a task at a system has what is in it, as the inspector has.
  const details = (tile: BoardTileView) => {
    const { facts, notes, rows } = inspector(tile, board.systems[tile.system_id], viewer, traits);
    return { facts, notes, rows };
  };
  const fleet =
    movement &&
    selectOrigin(update, movement.facts, movement.draft, open?.system_id ?? null, movement.handled);
  const origin = fleet && open ? { ...fleet, details: details(open) } : fleet;
  const tiles = map.map((tile) => tileView(tile, board.systems[tile.system_id], viewer, traits));
  // The landings with their controls take the place of the inspector of the active system.
  const sheet =
    landing && open?.system_id === landing.facts.active
      ? selectLanding(update, landing.facts, landing.draft, landing.chosen, catalog)
      : null;
  // The build with its controls takes the place of the inspector too, while it is chosen.
  const build =
    production && production.stage === "build" && open?.system_id === production.facts.system
      ? {
          ...selectBuild(update, production.facts, production.draft, "build"),
          details: details(open),
        }
      : null;
  return {
    tiles: move
      ? tiles.map((tile) => withMovement(tile, move))
      : landing
        ? tiles.map((tile) => withLandings(tile, landing.facts, landing.draft))
        : production
          ? tiles.map((tile) => withProduction(tile, production.facts, production.draft, viewer))
          : tiles,
    wormholes: marks.flatMap(([kind, id]) =>
      marks
        .filter(([other, otherId]) => other === kind && otherId > id)
        .map(([, otherId]): [string, string] => [id, otherId]),
    ),
    routes: move?.routes ?? [],
    activeSystem: board.active_system ?? null,
    inspected: open ? open.system_id : null,
    targeting: false,
    // The movement is staged on the board: a system that ships can leave opens its fleet.
    task: move
      ? {
          context: move.systems,
          target: "system",
          kind: "pick",
          interactive: true,
          values: Object.fromEntries(move.origins.map((id) => [id, 0])),
          chosen: origin ? { [origin.system]: true } : {},
          verb: "Move from",
          unit: "system",
        }
      : landing
        ? landingTask(landing.facts, landing.draft)
        : production
          ? productionTask(update, production.facts, production.draft, production.stage, !!build)
          : task,
    // The fleet with its controls takes the place of the inspector of that system.
    inspector:
      open && !origin && !sheet && !build
        ? {
            ...inspector(open, board.systems[open.system_id], viewer, traits),
            ...(movement && open.system_id === movement.facts.active
              ? {
                  arriving: selectArrival(
                    update,
                    movement.facts,
                    movement.draft,
                    movement.odds ?? null,
                  ),
                }
              : {}),
          }
        : null,
    origin: origin ?? null,
    landing: sheet,
    production: build,
    taskSystems: move
      ? move.systems
      : landing
        ? [landing.facts.active]
        : production
          ? production.stage === "build"
            ? [production.facts.system]
            : paymentSystems(update, production.facts)
          : task
            ? (task.highlight ?? Object.keys(task.values))
            : [],
    // The board is framed again for each movement: its systems are the task. The landings are
    // in the active system, which the movement had in view: the board stays as it is.
    fitKey: movement
      ? `movement:${movement.facts.active}`
      : landing
        ? `movement:${landing.facts.active}`
        : production
          ? // The build is in the active system, where the board is. The payment is anywhere.
            production.stage === "build"
            ? `movement:${production.facts.system}`
            : `payment:${production.facts.system}`
          : "local",
  };
}
