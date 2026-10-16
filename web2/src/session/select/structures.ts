// The placement of structures (`details.structures`): the game asks one structure at a time, and
// the first question already names every placement of the card from it on (Construction's
// primary: a space dock or a PDS, then a PDS; Thunder's Edge: a structure or a PRODUCTION, then
// a structure), with what a chosen option of a placement takes away from the next one. So the
// question that comes first is one screen with all of them: the window of a secondary (`yes` and
// `no`), the choice of an ability (`structure` and `produce|<system>`), or a placement itself.
// The system is chosen on the map; the panel then lists its planets, each with the structures
// that the open placement offers there, and the main button answers every question at once, by
// the ids of their options.
import type { BlockView, BoardTaskView, MenuRowView, RowView } from "../../model";
import { plural } from "../../model";
import type {
  Choice,
  SessionUpdate,
  StructureHints,
  StructurePlacement,
  StructureSpot,
} from "../wire";
import { isFollowWindow, secondaryCost } from "./follow";
import { systemLabel, tilesOf } from "./movement";
import { planetNames } from "./payment";
import { type Editor, answered, changeControl, hintOf } from "./editor";

/** What the player staged: the ability, the system in the panel, and the option of each placement. */
export interface StructureDraft {
  ability: string | null;
  /** The system whose planets the panel lists. */
  system: string | null;
  spots: (string | null)[];
}

export const NO_STRUCTURES: StructureDraft = { ability: null, system: null, spots: [] };

interface Question {
  /** A placement first, the window of a secondary first, or the choice of an ability first. */
  mode: "place" | "follow" | "ability";
  /** Every placement from this question on, as the game will offer it. */
  placements: StructurePlacement[];
  /** The player's structures on each planet of an option, by kind. */
  planets: StructureHints["planets"];
  /** The structures of each kind on the board, and the box. */
  supply: StructureHints["supply"];
  /** The abilities of the question: `structure`, then a production in a system. */
  abilities: { id: string; system: string | null }[];
}

/** The kinds in the order of the panel. */
const KINDS = ["spacedock", "pds"];
const NAME: Record<string, string> = { spacedock: "Space dock", pds: "PDS" };
const nameOf = (unit: string) => NAME[unit] ?? unit;
const lower = (unit: string) => (unit === "pds" ? "PDS" : nameOf(unit).toLowerCase());

function questionOf(choice: Choice): Question | null {
  const hints = hintOf<StructureHints>(choice, "structures");
  if (!hints?.placements.length) {
    return null;
  }
  const { placements, planets, supply } = hints;
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
  options: StructureSpot[][];
  spots: (StructureSpot | null)[];
  /** The first placement that is still open, with options. -1: none. */
  open: number;
  /** The system whose planets the panel lists, when a placement offers it. */
  system: string | null;
}

/** The staged choices that the game still offers, placement by placement. */
function stagedOf(question: Question, draft: StructureDraft | null): Staged {
  const staging = draft ?? NO_STRUCTURES;
  const placements = activeOf(question, staging);
  const options: StructureSpot[][] = [];
  const spots: (StructureSpot | null)[] = [];
  placements.forEach((placement, index) => {
    const earlier = index ? spots[index - 1] : null;
    const gone = new Set(earlier ? (placement.without[earlier.id] ?? []) : []);
    const offered = placement.options.filter((spot) => !gone.has(spot.id));
    options.push(offered);
    spots.push(offered.find((item) => item.id === staging.spots[index]) ?? null);
  });
  const open = spots.findIndex((spot, index) => !spot && options[index].length > 0);
  const system =
    staging.system &&
    placements.some((placement) => placement.options.some((spot) => spot.system === staging.system))
      ? staging.system
      : null;
  return { options, spots, open, system };
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
  const { options, spots, open, system } = stagedOf(question, { ...staging, ability });
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
  const labelOf = (index: number) => {
    const placement = placements[index];
    const kinds = KINDS.filter((unit) => placement.options.some((spot) => spot.unit === unit));
    const what = placement.only_pds
      ? "PDS only"
      : kinds.length > 1 || !kinds.length
        ? "space dock or PDS"
        : lower(kinds[0]);
    return placement.step !== null
      ? `${placement.step} of ${placement.of} · ${what}`
      : what.charAt(0).toUpperCase() + what.slice(1);
  };

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
  const rows: RowView[] = placements.map((_, index): RowView => {
    const spot = spots[index];
    if (!spot) {
      return {
        icon: "planet",
        title: [labelOf(index)],
        subtitle: options[index].length ? "No planet chosen" : "No planet to place it on",
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
      title: rows.length > 1 ? "Structures" : "Structure",
      aside: cost ?? undefined,
      rows,
    });
  }

  // The system of the map, then its planets with the structures of the open placement.
  if (placements.length && (open >= 0 || system)) {
    const offered = system ? options.flat().filter((spot) => spot.system === system).length : 0;
    blocks.push({
      kind: "card",
      title: "System",
      aside: "Choose on the map",
      rows: system
        ? [
            {
              icon: "token",
              title: [systemLabel(tiles, system)],
              subtitle: offered ? undefined : "No structure to place here",
              link: [`sys:${system}`],
              control: changeControl("system"),
            },
          ]
        : [{ icon: "token", title: ["No system chosen"] }],
    });
  }
  if (placements.length && system) {
    // Every planet of the system that a placement offers, in the order of the tile.
    const offering = new Set(
      placements.flatMap((placement) =>
        placement.options.filter((spot) => spot.system === system).map((spot) => spot.planet),
      ),
    );
    const order = (tiles.get(system)?.planets ?? []).map((planet) => planet.id);
    const listed = [
      ...order.filter((id) => offering.has(id)),
      ...[...offering].filter((id) => !order.includes(id)),
    ];
    const earlier = open > 0 ? spots[open - 1] : null;
    /** Why the open placement does not offer a kind on a planet. */
    const reason = (planet: string, unit: string) => {
      if (open < 0) {
        return "Every structure is placed";
      }
      const placement = placements[open];
      if (placement.only_pds && unit !== "pds") {
        return "PDS only";
      }
      const printed = placement.options.some(
        (spot) => spot.planet === planet && spot.unit === unit,
      );
      return printed && earlier
        ? `Not after the ${lower(earlier.unit)} on ${planetName(earlier.planet)}`
        : "Not offered";
    };
    const menu: MenuRowView[] = listed.flatMap((planet) =>
      KINDS.map((unit, index): MenuRowView => {
        const now = question.planets[planet]?.[unit] ?? 0;
        const here = spots.filter((spot) => spot?.planet === planet && spot.unit === unit).length;
        const offer =
          open >= 0
            ? options[open].find((spot) => spot.planet === planet && spot.unit === unit)
            : undefined;
        const stagedSpot = spots.find((spot) => spot?.planet === planet && spot.unit === unit);
        return {
          ...(index === 0 ? { group: planetName(planet) } : {}),
          key: String(key++),
          title: nameOf(unit),
          state: offer
            ? `${now} → ${now + here + 1}`
            : here
              ? `${now} → ${now + here}`
              : reason(planet, unit),
          selected: here > 0,
          disabled: !offer && !here,
          intent: {
            type: "chooseOption",
            option: offer?.id ?? stagedSpot?.id ?? `${unit}|${system}|${planet}`,
            group: "place",
          },
        };
      }),
    );
    blocks.push({
      kind: "menu",
      title:
        open < 0
          ? "Planets"
          : placements[open].step !== null
            ? `Structure ${labelOf(open)}`
            : "Structure",
      rows: menu,
    });
  }

  const chosen = spots.flatMap((spot) => (spot ? [spot.system] : []));
  const picking = open >= 0 && !system;
  const task: BoardTaskView | null = !placements.length
    ? null
    : picking
      ? {
          // The systems where the open placement offers a structure: the count of their planets.
          target: "system",
          kind: "pick",
          interactive: true,
          values: Object.fromEntries(
            [...new Set(options[open].map((spot) => spot.system))].map((id) => [
              id,
              new Set(options[open].filter((spot) => spot.system === id).map((spot) => spot.planet))
                .size,
            ]),
          ),
          chosen: Object.fromEntries(chosen.map((id) => [id, true])),
          verb: "Place a structure in",
          unit: "planets",
        }
      : {
          // The system is chosen: its planets are in the panel.
          target: "system",
          kind: "pick",
          interactive: false,
          values: Object.fromEntries([...chosen, ...(system ? [system] : [])].map((id) => [id, 0])),
          chosen: Object.fromEntries(chosen.map((id) => [id, true])),
          verb: "Placed in",
          unit: "planets",
          context: [...new Set([...chosen, ...(system ? [system] : [])])],
        };

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
  const next =
    open < 0 ? null : system ? "Choose a planet and a structure." : "Choose a system on the map.";
  const note =
    problem ??
    (passing && !system
      ? "Nothing chosen: you pass."
      : production
        ? `PRODUCTION · ${systemLabel(tiles, production.system!)}`
        : [...(summary ? [summary] : []), ...(next ? [next] : [])].join(" · ") ||
          "No structure to place");
  return {
    blocks,
    task,
    note,
    problem,
    label,
    changed: !!ability || placed.length > 0 || !!system,
    answers,
  };
}

/**
 * The structure of a planet (`place`, by the id of its option): the open placement places it,
 * or a staged one is taken back. Or the ability (`ability`).
 */
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
  if (group !== "place") {
    return null;
  }
  const { options, spots, open } = stagedOf(question, staging);
  const offer = open >= 0 ? options[open].find((spot) => spot.id === option) : undefined;
  if (offer) {
    const placed = [...staging.spots];
    placed[open] = offer.id;
    const after = stagedOf(question, { ...staging, spots: placed, system: offer.system });
    // The next placement has nothing in this system: its system is chosen on the map.
    const stays =
      after.open < 0 || after.options[after.open].some((spot) => spot.system === offer.system);
    return { ...staging, spots: placed, system: stays ? offer.system : null };
  }
  const last = spots.findLastIndex((spot) => spot?.id === option);
  if (last < 0) {
    return null;
  }
  const kept = [...staging.spots];
  kept[last] = null;
  return { ...staging, spots: kept };
}

/** A system of the map is chosen: the panel lists its planets. */
export function chooseStructureSystem(
  choice: Choice,
  draft: StructureDraft | null,
  system: string,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_STRUCTURES;
  const { options, open } = stagedOf(question, staging);
  if (open < 0 || !options[open].some((spot) => spot.system === system)) {
    return null;
  }
  return { ...staging, system };
}

/** A planet of the map is chosen: its system, as a click on the system. */
export function chooseStructurePlanet(
  choice: Choice,
  draft: StructureDraft | null,
  planet: string,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const { options, open } = stagedOf(question, draft);
  const spot = open >= 0 ? options[open].find((item) => item.planet === planet) : undefined;
  return spot ? chooseStructureSystem(choice, draft, spot.system) : null;
}

/**
 * "Change" of a placement (`structure:<index>`): it is open again, with the planets of its
 * system in the panel. "Change" of the system (`system`): it is chosen on the map again.
 */
export function clearStructurePart(
  choice: Choice,
  draft: StructureDraft | null,
  part: string,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question || !draft) {
    return null;
  }
  if (part === "system") {
    return draft.system ? { ...draft, system: null } : null;
  }
  const index = part.startsWith("structure:") ? Number(part.slice(10)) : Number.NaN;
  const spot = stagedOf(question, draft).spots[index];
  if (!spot) {
    return null;
  }
  const spots = [...draft.spots];
  spots[index] = null;
  return { ...draft, spots, system: spot.system };
}

/** Esc clears the last choice: the last planet, then the system, then the ability. */
export function clearLastStructure(
  choice: Choice,
  draft: StructureDraft | null,
): StructureDraft | null {
  const question = questionOf(choice);
  if (!question || !draft) {
    return null;
  }
  const { spots, system } = stagedOf(question, draft);
  const last = spots.findLastIndex((spot) => spot !== null);
  if (last >= 0) {
    const kept = [...draft.spots];
    kept[last] = null;
    return { ...draft, spots: kept };
  }
  if (system) {
    return { ...draft, system: null };
  }
  return draft.ability ? NO_STRUCTURES : null;
}

/** The placement of structures as an editor (`editors.ts`). */
export const structuresEditor: Editor<StructureDraft> = {
  view: (update, choice, draft) => answered(selectStructures(update, choice, draft)),
  option: chooseStructure,
  system: chooseStructureSystem,
  planet: chooseStructurePlanet,
  part: clearStructurePart,
  last: clearLastStructure,
};
