// Transit Diodes (crates/ti4-engine/src/technology.rs `transit_options`): one option per
// source place x ground force type x destination planet, id
// `transit|<source_system>|<source>|<unit>|<destination_system>|<planet>`, with the same parts in
// the payload. With a dozen planets that is well over a hundred options, so the client splits the
// pick into steps (what to move, then where to) and only ever submits one of the engine's ids.
import type { BoardView, ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { getUnitDisplayName } from "../components/UnitIcon.tsx";
import { isDeclineOption } from "./choiceModel.ts";
import { getPlanetDetails } from "./planetSelection.ts";
import { summarizeUnits, systemTag } from "./systemFacts.ts";

export interface TransitMove {
  optionId: string;
  sourceSystem: string;
  /** A planet id, or "space" for the space area. */
  source: string;
  unit: string;
  destinationSystem: string;
  planet: string;
}

export interface TransitDestination {
  optionId: string;
  systemId: string;
  planetId: string;
  name: string;
  resources: number | null;
  influence: number | null;
  /** The viewer's units on it now ("2 infantry, 1 space dock"), or null when empty. */
  here: string | null;
}

export interface TransitSystemGroup<T> {
  systemId: string;
  /** "#18 [Mecatol Rex]". */
  tag: string;
  items: T[];
}

/** One ground force type standing on one place, with every planet it may go to. */
export interface TransitForce {
  key: string;
  unit: string;
  /** "Infantry", "Mech". */
  unitName: string;
  /** How many of them stand there (from the board; at least 1). */
  count: number;
  destinations: TransitSystemGroup<TransitDestination>[];
}

/** A planet (or space area) the viewer's ground forces can leave from. */
export interface TransitSource {
  key: string;
  systemId: string;
  source: string;
  /** "Jord", or "Space area". */
  name: string;
  resources: number | null;
  influence: number | null;
  forces: TransitForce[];
}

export interface TransitModel {
  /** Ground forces still allowed this turn ("(3 left)" in the prompt), or null. */
  movesLeft: number | null;
  sources: TransitSystemGroup<TransitSource>[];
  /** Every move option, by id. */
  moves: Map<string, TransitMove>;
  declineOption: ChoiceOption | null;
}

const str = (v: unknown): string | null => (typeof v === "string" && v ? v : null);

/** Reads a transit option from its payload, falling back to the id parts. */
export function parseTransitOption(option: ChoiceOption): TransitMove | null {
  if (!option.id.startsWith("transit|")) return null;
  const p = option.payload ?? {};
  const parts = option.id.split("|");
  const sourceSystem = str(p.source_system) ?? parts[1];
  const source = str(p.source) ?? parts[2];
  const unit = str(p.unit) ?? parts[3];
  const destinationSystem = str(p.destination_system) ?? parts[4];
  const planet = str(p.planet) ?? parts.slice(5).join("|");
  if (!sourceSystem || !source || !unit || !destinationSystem || !planet) return null;
  return { optionId: option.id, sourceSystem, source, unit, destinationSystem, planet };
}

export function isTransitDiodesChoice(choice: Pick<PendingChoiceDto, "options" | "context">): boolean {
  const moves = choice.options.filter((o) => !isDeclineOption(o));
  if (moves.length === 0) return false;
  if (choice.context?.subtype === "transit_diodes_redeploy") return moves.every((o) => o.id.startsWith("transit|"));
  return moves.every((o) => parseTransitOption(o) !== null);
}

export const transitSourceKey = (systemId: string, source: string) => `${systemId}|${source}`;
export const transitForceKey = (systemId: string, source: string, unit: string) => `${systemId}|${source}|${unit}`;

/** Units named once and in the plural form the count needs ("Infantry", "Mechs"). */
export function transitUnitName(unit: string, count = 1): string {
  return `${getUnitDisplayName(unit, count)}${/2$/.test(unit) ? " II" : ""}`;
}

function groupBySystem<T>(entries: { systemId: string; item: T }[], board?: BoardView | null): TransitSystemGroup<T>[] {
  const groups = new Map<string, T[]>();
  for (const { systemId, item } of entries) groups.set(systemId, [...(groups.get(systemId) ?? []), item]);
  return [...groups].map(([systemId, items]) => ({ systemId, tag: systemTag(systemId, board), items }));
}

const byTag = <T>(a: TransitSystemGroup<T>, b: TransitSystemGroup<T>) =>
  a.tag.localeCompare(b.tag, undefined, { numeric: true });

function unitsOn(systemId: string, place: string, owner: string | null, board?: BoardView | null) {
  return (board?.systems?.[systemId]?.units ?? []).filter(
    (u) => (owner === null || u.owner === owner) && (place === "space" ? !u.planet : u.planet === place),
  );
}

export function buildTransitModel(
  choice: Pick<PendingChoiceDto, "options" | "prompt" | "actor">,
  board?: BoardView | null,
): TransitModel {
  const moves = new Map<string, TransitMove>();
  for (const option of choice.options) {
    const move = parseTransitOption(option);
    if (move) moves.set(move.optionId, move);
  }
  const left = /\((\d+) left\)/.exec(choice.prompt)?.[1];

  // source place -> unit -> moves, in option order
  const places = new Map<string, { systemId: string; source: string; units: Map<string, TransitMove[]> }>();
  for (const move of moves.values()) {
    const key = transitSourceKey(move.sourceSystem, move.source);
    const place = places.get(key) ?? { systemId: move.sourceSystem, source: move.source, units: new Map() };
    place.units.set(move.unit, [...(place.units.get(move.unit) ?? []), move]);
    places.set(key, place);
  }

  const sources = [...places].map(([key, place]) => {
    const details = place.source === "space" ? null : getPlanetDetails(place.source, board);
    const forces: TransitForce[] = [...place.units].map(([unit, list]) => {
      const count = Math.max(
        1,
        unitsOn(place.systemId, place.source, choice.actor, board).filter((u) => u.unit_type === unit).length,
      );
      const destinations = groupBySystem(
        list.map((move) => {
          const planet = getPlanetDetails(move.planet, board);
          const mine = unitsOn(move.destinationSystem, move.planet, choice.actor, board);
          return {
            systemId: move.destinationSystem,
            item: {
              optionId: move.optionId,
              systemId: move.destinationSystem,
              planetId: move.planet,
              name: planet.name,
              resources: planet.resources,
              influence: planet.influence,
              here: mine.length ? summarizeUnits(mine) : null,
            },
          };
        }),
        board,
      );
      // The source's own system first (the shortest hop to think about), then by tile number.
      destinations.sort((a, b) =>
        a.systemId === place.systemId ? -1 : b.systemId === place.systemId ? 1 : byTag(a, b),
      );
      return {
        key: transitForceKey(place.systemId, place.source, unit),
        unit,
        unitName: transitUnitName(unit, count),
        count,
        destinations,
      };
    });
    return {
      systemId: place.systemId,
      item: {
        key,
        systemId: place.systemId,
        source: place.source,
        name: details ? details.name : "Space area",
        resources: details?.resources ?? null,
        influence: details?.influence ?? null,
        forces,
      },
    };
  });

  return {
    movesLeft: left ? Number(left) : null,
    sources: groupBySystem(sources, board).sort(byTag),
    moves,
    declineOption: choice.options.find(isDeclineOption) ?? null,
  };
}

export function findTransitForce(model: TransitModel, key: string | null): { source: TransitSource; force: TransitForce } | null {
  if (!key) return null;
  for (const group of model.sources)
    for (const source of group.items)
      for (const force of source.forces) if (force.key === key) return { source, force };
  return null;
}

/** "Infantry from Jord in #18 [Mecatol Rex]" (one unit moves per answer). */
export function describeTransitSource(source: TransitSource, force: TransitForce, board?: BoardView | null): string {
  const from = source.source === "space" ? "the space area of" : `${source.name} in`;
  return `${transitUnitName(force.unit)} from ${from} ${systemTag(source.systemId, board)}`;
}

/** "Move 1 Infantry: Jord (#18 [Mecatol Rex]) → Lodor (#26 [Lodor])". */
export function describeTransitMove(move: TransitMove, board?: BoardView | null): string {
  const from =
    move.source === "space"
      ? `space area of ${systemTag(move.sourceSystem, board)}`
      : `${getPlanetDetails(move.source, board).name} (${systemTag(move.sourceSystem, board)})`;
  const to = `${getPlanetDetails(move.planet, board).name} (${systemTag(move.destinationSystem, board)})`;
  return `Move 1 ${transitUnitName(move.unit)}: ${from} → ${to}`;
}
