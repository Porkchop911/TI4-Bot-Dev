// The landings of an invasion from the facts of the engine and what is staged: the ground
// forces that can land, each planet of the active system, and the marks of this on the board.
import { planetValuesNow } from "@ti4/shared/planets/values.ts";
import type {
  BattleRowView,
  BattleSideView,
  BattleTableView,
  BoardTaskView,
  CargoSourceView,
  CombatOddsView,
  GroundOddsView,
  OddsView,
  InvasionView,
  LandingPlanetView,
  LandingView,
  TileView,
  UnitType,
} from "../../model";
import { GROUND_ODDS_COUNT } from "../../model";
import {
  type LandingDraft,
  chosenForce,
  forceId,
  forceLeft,
  landKey,
  landed,
} from "../landingDraft";
import { type GroundAsk, type KeptGroundOdds, groundAsks } from "../groundOdds";
import type {
  Catalog,
  ExplorationCatalog,
  GroundOddsResponse,
  GroundRoll,
  InvasionFacts,
  LandingPool,
  SessionUpdate,
} from "../wire";
import { NO_TRAITS, planetTraits } from "./exploration";
import { systemLabel, tilesOf } from "./movement";
import { byUnitOrder, forceOf, unitLabel, unitType } from "./names";
import { combatOdds } from "./odds";

/** The icon of a unit. A unit that the table of names does not have is shown as infantry. */
const typeOf = (unit: string): UnitType => unitType(unit) ?? "infantry";
/** The printed name of the units of a force: "Infantry II", "Mech (damaged)". */
const nameOf = (pool: LandingPool, printed: Catalog | null, count = 1) =>
  unitLabel(pool.unit, printed, count) + (pool.damaged ? " (damaged)" : "");

/** "2 Infantry I, 1 Mech": units of the forces, by the counts of `taken`. */
const unitsText = (
  facts: InvasionFacts,
  printed: Catalog | null,
  taken: (force: number) => number,
) =>
  facts.forces
    .flatMap((pool, force) => {
      const count = taken(force);
      return count ? [`${count} ${nameOf(pool, printed, count)}`] : [];
    })
    .join(", ");

/** The last answers of the engine for the odds of the ground combats, by planet. */
export type GroundOddsKept = Record<string, KeptGroundOdds>;

/** The share of the rollouts that a die hits: a die of 10 that needs `target` or more. */
const hitChance = (target: number) => Math.min(1, Math.max(0, (11 - target) / 10));

/**
 * The first round of the ground combat of an answer: what each kind of ground force of a side
 * rolls, with the hits that it is expected to score, and the odds in the headers.
 */
function previewOf(
  name: string,
  invader: string,
  viewer: string | null,
  defender: string,
  response: GroundOddsResponse,
  view: CombatOddsView,
  printed: Catalog | null,
): { table: BattleTableView; odds: OddsView } {
  const side = (
    seat: string,
    role: "Attacker" | "Defender",
    fielded: Record<string, number>,
    rolls: Record<string, GroundRoll>,
  ): BattleSideView => ({
    seat,
    role,
    isViewer: seat === viewer,
    hits: null,
    rows: Object.entries(fielded)
      .filter(([, count]) => count > 0)
      .map(([id, count]) => ({ id, count, unit: typeOf(id), roll: rolls[id] }))
      .sort((a, b) => byUnitOrder(a.id, b.id))
      .map(({ id, count, unit, roll }): BattleRowView => {
        const rolls = roll && roll.dice > 0;
        const target = rolls ? roll.hits_on - roll.modifier : null;
        const dice = rolls ? roll.dice * count : 0;
        return {
          unit,
          label: unitLabel(id, printed),
          count,
          target,
          targetTitle:
            rolls && roll.modifier !== 0
              ? `Printed ${roll.hits_on}+, ${roll.modifier > 0 ? "+" : "−"}${Math.abs(roll.modifier)} to each die`
              : "",
          modified: !!rolls && roll.modifier !== 0,
          dice,
          hits: target === null ? "" : `≈${(dice * hitChance(target)).toFixed(1)}`,
          scored: false,
          pills: [],
        };
      }),
  });
  const total = (counts: Record<string, number>) =>
    Object.values(counts).reduce((sum, count) => sum + count, 0);
  const left = (expected: Record<string, number>, fielded: Record<string, number>) =>
    `${total(expected).toFixed(1)} of ${total(fielded)}`;
  return {
    table: {
      label: `${name} · Ground combat · Round 1`,
      score: "Before the roll · expected hits",
      owed: null,
      sides: [
        side(invader, "Attacker", response.attacker_fielded, response.attacker_rolls),
        side(defender, "Defender", response.defender_fielded, response.defender_rolls),
      ],
    },
    odds: {
      kind: "ground",
      attacker: invader,
      defender,
      attackerWins: view.win ?? 0,
      defenderWins: view.loss ?? 0,
      rollouts: response.simulations,
      rounds: response.average_rounds.toFixed(1),
      attackerLeft: left(response.attacker_expected_survivors, response.attacker_fielded),
      defenderLeft: left(response.defender_expected_survivors, response.defender_fielded),
      caveat: GROUND_ODDS_COUNT,
    },
  };
}

/**
 * The odds of the ground combat on a planet. `kept` is the last answer of the engine for that
 * planet: of other landings it is shown as pending, with the units that fight now.
 */
function oddsOf(
  name: string,
  invader: string | undefined,
  viewer: string | null,
  ask: GroundAsk | undefined,
  kept: KeptGroundOdds | undefined,
  printed: Catalog | null,
): GroundOddsView | null {
  if (!ask || !invader) {
    return null;
  }
  if (ask.kind === "unavailable") {
    return {
      defender: ask.defender,
      win: null,
      draw: null,
      loss: null,
      rollouts: 0,
      pending: false,
      sides: [],
      unavailable: true,
      preview: null,
    };
  }
  const pending = kept?.key !== ask.ask.key;
  // The engine had no odds for these sides.
  if (!pending && !kept?.response) {
    return null;
  }
  const response = kept?.response ?? null;
  const { attacker, defender } = ask.ask.request;
  const view = combatOdds({
    attacker: invader,
    defender: ask.ask.defender,
    response,
    unresolved: "loss",
    pending,
    fielded: { attacker: attacker.units, defender: defender.units },
    printed,
  });
  // The table is of the answer: of its units, also while the answer for new landings is on its way.
  return {
    ...view,
    unavailable: false,
    preview:
      response && previewOf(name, invader, viewer, ask.ask.defender, response, view, printed),
  };
}

/** The landings of the active system: what the sheet on the board and the panel show. */
export function selectLanding(
  update: SessionUpdate,
  facts: InvasionFacts,
  draft: LandingDraft,
  chosen: number | null,
  catalog: ExplorationCatalog | null = null,
  /** The last answers of the engine for the odds of the ground combats. */
  odds: GroundOddsKept = {},
  /** The printed names of the units. */
  printed: Catalog | null = null,
): LandingView {
  const asks = new Map(groundAsks(update, facts, draft).map((ask) => [ask.planet, ask]));
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
      name: nameOf(pool, printed),
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
      ...planetValuesNow(info ?? { resources: 0, influence: 0 }, system?.planets[planet]),
      owner: system?.planets[planet]?.controlled_by ?? null,
      traits: info ? traits(info) : NO_TRAITS,
      there: [...new Set(units.map((unit) => unit.owner))].map((seat) => ({
        seat,
        force: forceOf(
          units.filter((unit) => unit.owner === seat),
          printed,
        ),
      })),
      tokens: Object.entries(staged).flatMap(([force, count]) => {
        const pool = facts.forces[Number(force)];
        return Array.from({ length: count }, () => ({
          key: landKey(planet, Number(force)),
          unit: typeOf(pool.unit),
          name: nameOf(pool, printed),
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
      lands: unitsText(facts, printed, (force) => staged[force] ?? 0),
      reason: lands.length ? null : "Ground forces cannot land here",
      odds: oddsOf(
        info?.label ?? planet,
        update.view.board.invasion?.invader,
        update.viewer.role === "player" ? update.viewer.seat : null,
        asks.get(planet),
        odds[planet],
        printed,
      ),
    };
  });
  return {
    system: facts.active,
    label: systemLabel(tiles, facts.active),
    forces,
    chosen: pick === null ? null : forceId(pick),
    planets,
    landed: landed(draft),
    stays: unitsText(facts, printed, (force) => forceLeft(facts, draft, force)),
  };
}

/** The invasion step while the landings are staged. The controls are on the board. */
export const selectInvasion = (
  update: SessionUpdate,
  facts: InvasionFacts,
  draft: LandingDraft,
  chosen: number | null,
  catalog: ExplorationCatalog | null = null,
  odds: GroundOddsKept = {},
  printed: Catalog | null = null,
): InvasionView => ({
  kind: "invasion",
  landing: selectLanding(update, facts, draft, chosen, catalog, odds, printed),
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
