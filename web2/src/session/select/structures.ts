// The placement of structures (`details.structures`): the game asks one structure at a time, and
// the first question already names every placement of the card from it on (Construction's
// primary: a space dock or a PDS, then a PDS; Thunder's Edge: a structure or a PRODUCTION, then
// a structure), with what a chosen option of a placement takes away from the next one. So the
// question that comes first is one screen with all of them: the window of a secondary (`yes` and
// `no`), the choice of an ability (`structure` and `produce|<system>`), or a placement itself.
// The kind of structure is chosen in the panel, the planet on the map, and the main button
// answers every question at once, by the ids of their options.
import type { BlockView, BoardTaskView, MenuRowView, RowView } from "../../model";
import { plural } from "../../model";
import type { Choice, SessionUpdate } from "../wire";
import { isFollowWindow, secondaryCost } from "./follow";
import { systemLabel, tilesOf } from "./movement";
import { planetNames } from "./payment";
import { type Editor, answered, changeControl, num } from "./editor";

/** What the player staged: the ability, and the kind and the option of each placement. */
export interface StructureDraft {
  ability: string | null;
  units: (string | null)[];
  spots: (string | null)[];
}

export const NO_STRUCTURES: StructureDraft = { ability: null, units: [], spots: [] };

interface Spot {
  id: string;
  unit: string;
  planet: string;
  system: string;
}

interface Placement {
  step: number | null;
  of: number | null;
  onlyPds: boolean;
  options: Spot[];
  /** The options of this placement that an option of the placement before it takes away. */
  without: Record<string, string[]>;
}

interface Question {
  /** A placement first, the window of a secondary first, or the choice of an ability first. */
  mode: "place" | "follow" | "ability";
  placements: Placement[];
  /** The player's structures on each planet of an option, by kind. */
  planets: Record<string, Record<string, number>>;
  /** The structures of each kind on the board, and the box. */
  supply: Record<string, { placed: number; limit: number }>;
  /** The abilities of the question: `structure`, then a production in a system. */
  abilities: { id: string; system: string | null }[];
}

/** The kinds in the order of the panel. */
const KINDS = ["spacedock", "pds"];
const NAME: Record<string, string> = { spacedock: "Space dock", pds: "PDS" };
const nameOf = (unit: string) => NAME[unit] ?? unit;
const lower = (unit: string) => (unit === "pds" ? "PDS" : nameOf(unit).toLowerCase());

const str = (value: unknown): string => (typeof value === "string" ? value : "");

function spotsOf(value: unknown): Spot[] {
  return (Array.isArray(value) ? value : []).flatMap((item: Record<string, unknown>): Spot[] =>
    typeof item?.id === "string" && typeof item.unit === "string"
      ? [{ id: item.id, unit: item.unit, planet: str(item.planet), system: str(item.system) }]
      : [],
  );
}

function questionOf(choice: Choice): Question | null {
  const details = choice.details ?? {};
  const hints = details.structures as Record<string, unknown> | undefined;
  if (!hints || !Array.isArray(hints.placements) || !hints.placements.length) {
    return null;
  }
  const placements = (hints.placements as Record<string, unknown>[]).map((item): Placement => ({
    step: num(item?.step),
    of: num(item?.of),
    onlyPds: item?.only_pds === true,
    options: spotsOf(item?.options),
    without: Object.fromEntries(
      Object.entries((item?.without ?? {}) as Record<string, unknown>).map(([id, gone]) => [
        id,
        Array.isArray(gone) ? gone.filter((one): one is string => typeof one === "string") : [],
      ]),
    ),
  }));
  const planets = (hints.planets ?? {}) as Record<string, Record<string, number>>;
  const supply = Object.fromEntries(
    Object.entries((hints.supply ?? {}) as Record<string, Record<string, unknown>>).flatMap(
      ([unit, item]) => {
        const placed = num(item?.placed);
        const limit = num(item?.limit);
        return placed !== null && limit !== null ? [[unit, { placed, limit }]] : [];
      },
    ),
  );
  const ids = new Set(choice.options.map((option) => option.id));
  const base = { placements, planets, supply, abilities: [] };
  if (isFollowWindow(choice)) {
    return { ...base, mode: "follow" };
  }
  if (ids.has("structure")) {
    return {
      ...base,
      mode: "ability",
      abilities: choice.options.map((option) => ({
        id: option.id,
        system: option.id.startsWith("produce|") ? option.id.slice(8) : null,
      })),
    };
  }
  const placing = choice.options.filter((option) => option.kind !== "decline");
  if (placing.length && placing.every((option) => typeof option.payload?.unit === "string")) {
    return { ...base, mode: "place" };
  }
  return null;
}

/** The placements that the staged ability places: all of them, or none for a production. */
const activeOf = (question: Question, draft: StructureDraft) =>
  question.mode !== "ability" || draft.ability === "structure" ? question.placements : [];

interface Staged {
  /** The options of each placement, after what the placement before it staged. */
  options: Spot[][];
  units: (string | null)[];
  spots: (Spot | null)[];
  /** The first placement that is still open, with options. -1: none. */
  open: number;
}

/** The staged choices that the game still offers, placement by placement. */
function stagedOf(question: Question, draft: StructureDraft | null): Staged {
  const staging = draft ?? NO_STRUCTURES;
  const placements = activeOf(question, staging);
  const options: Spot[][] = [];
  const units: (string | null)[] = [];
  const spots: (Spot | null)[] = [];
  placements.forEach((placement, index) => {
    const earlier = index ? spots[index - 1] : null;
    const gone = new Set(earlier ? (placement.without[earlier.id] ?? []) : []);
    const offered = placement.options.filter((spot) => !gone.has(spot.id));
    const kinds = KINDS.filter((unit) => offered.some((spot) => spot.unit === unit));
    const wanted = staging.units[index] ?? null;
    const unit = wanted && kinds.includes(wanted) ? wanted : kinds.length === 1 ? kinds[0] : null;
    const spot = offered.find((item) => item.id === staging.spots[index]) ?? null;
    options.push(offered);
    units.push(spot ? spot.unit : unit);
    spots.push(spot);
  });
  const open = spots.findIndex((spot, index) => !spot && options[index].length > 0);
  return { options, units, spots, open };
}

export interface StructuresView {
  blocks: BlockView[];
  task: BoardTaskView | null;
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** The answers, by the ids of their options, in the order that the game asks. Null: not yet. */
  answers: string[] | null;
}

/** The placement of structures, with its window or its ability. Null for any other question. */
export function selectStructures(
  update: SessionUpdate,
  choice: Choice,
  draft: StructureDraft | null,
): StructuresView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_STRUCTURES;
  const ability = question.abilities.some((item) => item.id === staging.ability)
    ? staging.ability
    : null;
  const { options, units, spots, open } = stagedOf(question, { ...staging, ability });
  const placements = activeOf(question, { ...staging, ability });
  const tiles = tilesOf(update);
  const planets = planetNames(update);
  const planetName = (id: string) => planets.get(id) ?? id;
  // What the secondary costs. The game does not say who makes it free.
  const cost =
    question.mode === "follow" && choice.details?.costs_token === true
      ? secondaryCost(choice.details)
      : null;
  /** The structures of a kind on a planet now, and with what is staged before `index`. */
  const onPlanet = (planet: string, unit: string, index: number) => {
    const now = question.planets[planet]?.[unit] ?? 0;
    const staged = spots
      .slice(0, index)
      .filter((spot) => spot?.planet === planet && spot.unit === unit).length;
    return now + staged;
  };
  const stagedOfKind = (unit: string) => spots.filter((spot) => spot?.unit === unit).length;

  const blocks: BlockView[] = [];
  let key = 1;
  if (question.mode === "ability") {
    blocks.push({
      kind: "menu",
      title: "Ability",
      rows: question.abilities.map((item): MenuRowView => ({
        key: String(key++),
        title: item.system
          ? `Use PRODUCTION · ${systemLabel(tiles, item.system)}`
          : "Place a structure",
        selected: ability === item.id,
        disabled: false,
        intent: { type: "chooseOption", option: item.id, group: "ability" },
      })),
    });
  }
  const supplied = KINDS.filter((unit) => question.supply[unit]);
  if (placements.length && supplied.length) {
    blocks.push({
      kind: "gauges",
      gauges: supplied.map((unit) => ({
        label: unit === "pds" ? "PDS on the board" : "Space docks on the board",
        used: question.supply[unit].placed + stagedOfKind(unit),
        total: question.supply[unit].limit,
      })),
    });
  }
  // The kind of structure of the open placement, when it can be more than one.
  const kindsOf = (index: number) =>
    KINDS.filter((unit) => options[index].some((spot) => spot.unit === unit));
  if (open >= 0 && kindsOf(open).length > 1) {
    const placement = placements[open];
    blocks.push({
      kind: "menu",
      title:
        placement.step !== null ? `Structure ${placement.step} of ${placement.of}` : "Structure",
      rows: KINDS.map((unit): MenuRowView => {
        const box = question.supply[unit];
        return {
          key: String(key++),
          title: nameOf(unit),
          state: box ? `${box.limit - box.placed - stagedOfKind(unit)} in your reinforcements` : "",
          selected: units[open] === unit,
          disabled: !kindsOf(open).includes(unit),
          intent: { type: "chooseOption", option: unit, group: `structure:${open}` },
        };
      }),
    });
  }
  const labelOf = (index: number) => {
    const placement = placements[index];
    const kinds = KINDS.filter((unit) => placement.options.some((spot) => spot.unit === unit));
    const what = placement.onlyPds
      ? "PDS only"
      : kinds.length > 1 || !kinds.length
        ? "space dock or PDS"
        : lower(kinds[0]);
    return placement.step !== null
      ? `${placement.step} of ${placement.of} · ${what}`
      : what.charAt(0).toUpperCase() + what.slice(1);
  };
  const rows: RowView[] = placements.map((_, index): RowView => {
    const spot = spots[index];
    if (!spot) {
      return {
        icon: "planet",
        title: [labelOf(index)],
        subtitle: !options[index].length
          ? "No planet to place it on"
          : index !== open || units[index]
            ? "No planet chosen"
            : "Choose the structure first",
      };
    }
    const now = onPlanet(spot.planet, spot.unit, index);
    return {
      icon: "planet",
      title: [`${planetName(spot.planet)} · #${spot.system}`],
      subtitle: `${nameOf(spot.unit)} ${now} → ${now + 1}`,
      link: [`pl:${spot.planet}`, `sys:${spot.system}`],
      control: changeControl(`structure:${index}`),
    };
  });
  const production = question.abilities.find((item) => item.id === ability && item.system);
  if (production) {
    // The second structure is asked after the production.
    const after = question.placements.at(-1);
    rows.push({
      icon: "planet",
      title: [
        after?.step !== null && after ? `${after.step} of ${after.of} · structure` : "Structure",
      ],
      subtitle: "After the production",
    });
  }
  if (rows.length) {
    blocks.push({
      kind: "card",
      title: rows.length > 1 ? "Structures" : "Planet",
      aside: [...(placements.length ? ["Choose on the map"] : []), ...(cost ? [cost] : [])].join(
        " · ",
      ),
      rows,
    });
  }

  const unit = open >= 0 ? units[open] : null;
  const pickable = open >= 0 && unit ? options[open].filter((spot) => spot.unit === unit) : [];
  const chosen = spots.flatMap((spot) => (spot ? [spot.planet] : []));
  const values = Object.fromEntries([
    ...chosen.map((planet) => [planet, 0] as const),
    ...pickable.map((spot) => [spot.planet, onPlanet(spot.planet, spot.unit, open)] as const),
  ]);
  const task: BoardTaskView | null =
    placements.length && Object.keys(values).length
      ? {
          target: "planet",
          kind: "pick",
          interactive: true,
          values,
          chosen: Object.fromEntries(chosen.map((planet) => [planet, true])),
          verb: unit ? `Place ${lower(unit)} on` : "Placed on",
          unit: unit === "pds" ? "PDS there" : unit ? "space docks there" : "structures there",
        }
      : null;

  const placed = spots.flatMap((spot) => (spot ? [spot] : []));
  const summary = placed
    .map((spot) => `${nameOf(spot.unit)} on ${planetName(spot.planet)}`)
    .join(" · ");
  const passing = question.mode === "follow" && !placed.length;
  const problem =
    question.mode === "ability" && !ability
      ? "Choose an ability: a structure or a PRODUCTION."
      : null;
  /** Each placement that the game asks: the staged option, or none. */
  const placing = placements.flatMap((_, index) =>
    options[index].length ? [spots[index]?.id ?? "decline"] : [],
  );
  const answers = problem
    ? null
    : question.mode === "follow"
      ? passing
        ? ["no"]
        : ["yes", ...placing]
      : question.mode === "ability"
        ? production
          ? [production.id]
          : ["structure", ...placing]
        : placing;
  const primary = question.placements[0]?.step !== null;
  const label =
    question.mode === "follow"
      ? passing
        ? "Pass"
        : `Follow · place ${placed.map((spot) => lower(spot.unit)).join(" and ")}`
      : production
        ? "Resolve primary · use PRODUCTION"
        : primary
          ? placed.length === placements.filter((_, index) => options[index].length).length
            ? "Resolve primary"
            : `Resolve primary · ${placed.length ? plural(placed.length, "structure") : "no structure"}`
          : placed.length
            ? `Place ${placed.map((spot) => lower(spot.unit)).join(" and ")}`
            : "Place nothing";
  const note =
    problem ??
    (passing
      ? "Nothing chosen: you pass."
      : production
        ? `PRODUCTION · ${systemLabel(tiles, production.system!)}`
        : open >= 0
          ? [
              ...(summary ? [summary] : []),
              unit
                ? "Choose a planet on the map."
                : "Choose a structure, then a planet on the map.",
            ].join(" · ")
          : summary || "No structure to place");
  return {
    blocks,
    task,
    note,
    problem,
    label,
    changed: !!ability || placed.length > 0 || staging.units.some(Boolean),
    answers,
  };
}

/** The kind of structure of a placement (`structure:<index>`), or the ability (`ability`). */
export function chooseStructure(
  choice: Choice,
  draft: StructureDraft | null,
  group: string | undefined,
  option: string,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_STRUCTURES;
  if (group === "ability") {
    if (!question.abilities.some((item) => item.id === option)) {
      return null;
    }
    return { ...staging, ability: staging.ability === option ? null : option };
  }
  const index = group?.startsWith("structure:") ? Number(group.slice(10)) : Number.NaN;
  const { options, spots } = stagedOf(question, staging);
  if (!options[index]?.some((spot) => spot.unit === option) || spots[index]) {
    return null;
  }
  const units = [...staging.units];
  units[index] = units[index] === option ? null : option;
  return { ...staging, units };
}

/** A planet of the map is chosen: the open placement places its kind there. */
export function placeStructure(
  choice: Choice,
  draft: StructureDraft | null,
  planet: string,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_STRUCTURES;
  const { options, units, open } = stagedOf(question, staging);
  const spot =
    open >= 0 && units[open]
      ? options[open].find((item) => item.planet === planet && item.unit === units[open])
      : undefined;
  if (!spot) {
    return null;
  }
  const spots = [...staging.spots];
  spots[open] = spot.id;
  const kinds = [...staging.units];
  kinds[open] = spot.unit;
  return { ...staging, units: kinds, spots };
}

/** "Change" of a placement (`structure:<index>`): its planet is chosen again. */
export function clearStructurePart(
  choice: Choice,
  draft: StructureDraft | null,
  part: string,
): StructureDraft | null {
  const index = part.startsWith("structure:") ? Number(part.slice(10)) : Number.NaN;
  if (!questionOf(choice) || !draft?.spots[index]) {
    return null;
  }
  const spots = [...draft.spots];
  spots[index] = null;
  return { ...draft, spots };
}

/** Esc clears the last choice: the last planet, then the kinds, then the ability. */
export function clearLastStructure(
  choice: Choice,
  draft: StructureDraft | null,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question || !draft) {
    return null;
  }
  const { spots } = stagedOf(question, draft);
  const last = spots.findLastIndex((spot) => spot !== null);
  if (last >= 0) {
    const kept = [...draft.spots];
    kept[last] = null;
    return { ...draft, spots: kept };
  }
  if (draft.units.some(Boolean)) {
    return { ...draft, units: [] };
  }
  return draft.ability ? NO_STRUCTURES : null;
}

/** The placement of structures as an editor (`editors.ts`). */
export const structuresEditor: Editor<StructureDraft> = {
  view: (update, choice, draft) => answered(selectStructures(update, choice, draft)),
  option: chooseStructure,
  planet: placeStructure,
  part: clearStructurePart,
  last: clearLastStructure,
};
