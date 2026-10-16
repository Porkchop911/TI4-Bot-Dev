// The landings of an invasion from the facts of the engine and what is staged: the ground
// forces that can land, each planet of the active system, and the marks of this on the board.
import type {
  BoardTaskView,
  CargoSourceView,
  InvasionView,
  LandingPlanetView,
  LandingView,
  TileView,
  UnitType,
} from "../../model";
import { unitName } from "../../model";
import {
  type LandingDraft,
  chosenForce,
  forceId,
  forceLeft,
  landKey,
  landed,
} from "../landingDraft";
import type { ExplorationCatalog, InvasionFacts, LandingPool, SessionUpdate } from "../wire";
import { NO_TRAITS, planetTraits } from "./exploration";
import { systemLabel, tilesOf } from "./movement";
import { forceOf, unitType } from "./names";

/** The icon of a unit. A unit that the table of names does not have is shown as infantry. */
const typeOf = (unit: string): UnitType => unitType(unit) ?? "infantry";
const nameOf = (pool: LandingPool, count = 1) =>
  unitName(typeOf(pool.unit), count) + (pool.damaged ? " (damaged)" : "");

/** "2 Infantry, 1 Mech": units of the forces, by the counts of `taken`. */
const unitsText = (facts: InvasionFacts, taken: (force: number) => number) =>
  facts.forces
    .flatMap((pool, force) => {
      const count = taken(force);
      return count ? [`${count} ${nameOf(pool, count)}`] : [];
    })
    .join(", ");

/** The landings of the active system: what the sheet on the board and the panel show. */
export function selectLanding(
  update: SessionUpdate,
  facts: InvasionFacts,
  draft: LandingDraft,
  chosen: number | null,
  catalog: ExplorationCatalog | null = null,
): LandingView {
  const traits = planetTraits(update, catalog);
  const tiles = tilesOf(update);
  const planetName = (system: string, planet: string) =>
    tiles.get(system)?.planets?.find((item) => item.id === planet)?.label ?? planet;
  const system = update.view.board.systems[facts.active];
  const meta = tiles.get(facts.active)?.planets ?? [];
  const forces = facts.forces.map((pool, force): CargoSourceView => {
    const from = pool.from_system ?? facts.active;
    return {
      id: forceId(force),
      unit: typeOf(pool.unit),
      name: nameOf(pool),
      place: pool.from_planet ? `Planet ${planetName(from, pool.from_planet)}` : "Space area",
      site: from === facts.active ? null : { system: from, label: systemLabel(tiles, from) },
      left: forceLeft(facts, draft, force),
      reason: null,
    };
  });
  const pick = chosenForce(facts, draft, chosen);
  const planets = facts.planets.map(({ planet, lands = [] }): LandingPlanetView => {
    const info = meta.find((item) => item.id === planet);
    const staged = draft[planet] ?? {};
    const units = (system?.units ?? []).filter((unit) => unit.planet === planet);
    return {
      id: planet,
      name: info?.label ?? planet,
      resources: info?.resources ?? 0,
      influence: info?.influence ?? 0,
      owner: system?.planets[planet]?.controlled_by ?? null,
      traits: info ? traits(info) : NO_TRAITS,
      there: [...new Set(units.map((unit) => unit.owner))].map((seat) => ({
        seat,
        force: forceOf(units.filter((unit) => unit.owner === seat)),
      })),
      tokens: Object.entries(staged).flatMap(([force, count]) => {
        const pool = facts.forces[Number(force)];
        return Array.from({ length: count }, () => ({
          key: landKey(planet, Number(force)),
          unit: typeOf(pool.unit),
          name: nameOf(pool),
          value: count - 1,
        }));
      }),
      accepts: Object.fromEntries(
        lands
          .filter((force) => forceLeft(facts, draft, force) > 0)
          .map((force) => [
            forceId(force),
            { key: landKey(planet, force), value: (staged[force] ?? 0) + 1 },
          ]),
      ),
      lands: unitsText(facts, (force) => staged[force] ?? 0),
      reason: lands.length ? null : "Ground forces cannot land here",
    };
  });
  return {
    system: facts.active,
    label: systemLabel(tiles, facts.active),
    forces,
    chosen: pick === null ? null : forceId(pick),
    planets,
    landed: landed(draft),
    stays: unitsText(facts, (force) => forceLeft(facts, draft, force)),
  };
}

/** The invasion step while the landings are staged. The controls are on the board. */
export const selectInvasion = (
  update: SessionUpdate,
  facts: InvasionFacts,
  draft: LandingDraft,
  chosen: number | null,
  catalog: ExplorationCatalog | null = null,
): InvasionView => ({
  kind: "invasion",
  landing: selectLanding(update, facts, draft, chosen, catalog),
  skipped: null,
  planets: [],
  gauges: [],
  plannedNote: false,
  revision: "",
});

/** The planets of the active system are the choice on the board: a tap lands one unit there. */
export const landingTask = (facts: InvasionFacts, draft: LandingDraft): BoardTaskView => ({
  target: "planet",
  kind: "pick",
  interactive: true,
  values: Object.fromEntries(
    facts.planets
      .filter((item) => item.lands?.length)
      .map((item) => [
        item.planet,
        Object.values(draft[item.planet] ?? {}).reduce((sum, count) => sum + count, 0),
      ]),
  ),
  chosen: Object.fromEntries(Object.keys(draft).map((planet) => [planet, true])),
  verb: "Land on",
  unit: "landing",
  context: [facts.active],
});

/** A tile with the ground forces of its planets after the staged landings. */
export function withLandings(tile: TileView, facts: InvasionFacts, draft: LandingDraft): TileView {
  // A force that lands from a planet leaves that planet, in the active system or another one.
  const leaves = (planet: string) =>
    facts.forces.reduce(
      (sum, pool, force) =>
        pool.from_planet === planet && (pool.from_system ?? facts.active) === tile.id
          ? sum + pool.count - forceLeft(facts, draft, force)
          : sum,
      0,
    );
  const arrives = (planet: string) =>
    tile.id === facts.active
      ? Object.values(draft[planet] ?? {}).reduce((sum, count) => sum + count, 0)
      : 0;
  return {
    ...tile,
    planets: tile.planets.map((planet) => ({
      ...planet,
      groundForces: planet.groundForces + arrives(planet.id) - leaves(planet.id),
    })),
  };
}
