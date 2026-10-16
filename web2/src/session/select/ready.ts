// The readying of exhausted planets (`details.ready_hints` with `details.ready_count`), with what
// comes before it in the same question: the system of a card that readies after it (Diplomacy's
// primary: options of kind `system`), or the window of a secondary (`yes` and `no`). The game
// asks the system, the window and each planet one at a time; here they are one screen: the
// system and the planets are chosen on the map, and the main button answers all of them at once,
// by the ids of their options.
import type { BlockView, BoardTaskView } from "../../model";
import { plural } from "../../model";
import type {
  Catalog,
  Choice,
  ReadyHints,
  ReadyPlanet,
  ReadySpender,
  SessionUpdate,
} from "../wire";
import { systemLabel, tilesOf } from "./movement";
import { isFollowWindow, secondaryCost } from "./follow";
import { seatLabel } from "./names";
import { type Editor, answered, changeControl, hintFieldsOf } from "./editor";

/** What the player staged: the system, and the planets in the order that they were chosen. */
export interface ReadyDraft {
  system: string | null;
  planets: string[];
}

export const NO_READY: ReadyDraft = { system: null, planets: [] };

interface Question {
  /** A system first, the window of a secondary first, or only planets. */
  mode: "system" | "follow" | "ready";
  systems: string[];
  /** The exhausted planets that can be readied. */
  planets: ReadyPlanet[];
  /** How many planets the game readies at most, with this question and the ones after it. */
  count: number;
  spenders: ReadySpender[];
}

function questionOf(choice: Choice): Question | null {
  const told = hintFieldsOf<ReadyHints>(choice, "ready_hints", "ready_count");
  if (!told) {
    return null;
  }
  const hinted = told.ready_hints.planets;
  const spenders = told.ready_hints.spenders;
  const count = told.ready_count;
  const options = choice.options.filter((option) => option.kind !== "decline");
  if (options.length && options.every((option) => option.kind === "system")) {
    return {
      mode: "system",
      systems: options.map((option) => option.id),
      planets: hinted,
      count,
      spenders,
    };
  }
  if (isFollowWindow(choice)) {
    return { mode: "follow", systems: [], planets: hinted, count, spenders };
  }
  if (options.length && options.every((option) => option.kind === "ready")) {
    // The options are the planets: the hints say what each is worth.
    return {
      mode: "ready",
      systems: [],
      planets: options.map(
        (option) =>
          hinted.find((planet) => planet.id === option.id) ?? {
            id: option.id,
            name: option.id,
            resources: null,
            influence: null,
          },
      ),
      count,
      spenders,
    };
  }
  return null;
}

export interface ReadyView {
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

/** "A, B and 2 more" */
const names = (list: string[]) =>
  list.length > 3
    ? `${list.slice(0, 2).join(", ")} and ${list.length - 2} more`
    : list.length < 2
      ? (list[0] ?? "")
      : `${list.slice(0, -1).join(", ")} and ${list.at(-1)}`;

/** The staged choices that the question still offers. */
function stagedOf(question: Question, draft: ReadyDraft | null) {
  const staging = draft ?? NO_READY;
  const need = Math.min(question.count, question.planets.length);
  const system =
    question.mode === "system" && staging.system && question.systems.includes(staging.system)
      ? staging.system
      : null;
  const planets = staging.planets
    .filter((id) => question.planets.some((planet) => planet.id === id))
    .slice(0, need);
  return { system, planets, need };
}

/** The readying of planets, with its system or its window. Null for any other question. */
export function selectReady(
  update: SessionUpdate,
  choice: Choice,
  draft: ReadyDraft | null,
  printed: Catalog | null = null,
): ReadyView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const { system, planets, need } = stagedOf(question, draft);
  const tiles = tilesOf(update);
  const systemOf = (planet: string) =>
    (update.view.board.map_tiles ?? []).find((tile) =>
      (tile.planets ?? []).some((item) => item.id === planet),
    )?.system_id;
  const planetOf = (id: string) => question.planets.find((planet) => planet.id === id)!;
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  // What the secondary costs. The game does not say who makes it free.
  const cost =
    question.mode === "follow" && choice.details?.costs_token === true
      ? secondaryCost(choice.details)
      : null;

  const blocks: BlockView[] = [];
  if (question.mode === "system") {
    const others = update.view.players
      .filter((player) => player.id !== viewer)
      .map((player) => seatLabel(update, player.id, printed));
    blocks.push({
      kind: "card",
      title: "System",
      aside: "Choose on the map",
      rows: system
        ? [
            {
              icon: "token",
              title: [systemLabel(tiles, system)],
              subtitle: `${names(others)} ${others.length === 1 ? "places" : "place"} a command token here`,
              link: [`sys:${system}`],
              control: changeControl("system"),
            },
          ]
        : [{ icon: "token", title: ["No system chosen"] }],
    });
  }
  const worth = (planet: ReadyPlanet) =>
    planet.resources !== null && planet.influence !== null
      ? ` · ${plural(planet.resources, "resource")} / ${planet.influence} influence`
      : "";
  blocks.push({
    kind: "card",
    title: "Ready planets",
    aside: [
      `${planets.length} / ${need}`,
      question.mode === "system" && !system ? "after the system" : "Choose on the map",
      ...(cost ? [cost] : []),
    ].join(" · "),
    rows: planets.length
      ? planets.map((id) => {
          const where = systemOf(id);
          return {
            icon: "planet" as const,
            title: [where ? `${planetOf(id).name} · #${where}` : planetOf(id).name],
            subtitle: `Exhausted → ready${worth(planetOf(id))}`,
            link: where ? [`pl:${id}`, `sys:${where}`] : [`pl:${id}`],
            control: changeControl(`planet:${id}`),
          };
        })
      : [{ icon: "planet", title: [need ? "No planet chosen" : "No exhausted planet"] }],
  });
  // The cards of this round that spend what a ready planet gives: public, on the table.
  const spent = question.spenders.filter((item) => item.status !== "not_in_play");
  if (spent.length) {
    blocks.push({
      kind: "note",
      tone: "quiet",
      strong: "This round",
      text: spent
        .map(
          (item) =>
            `${item.card} ${item.status === "played" ? "played" : "to come"}${item.spends ? ` (${item.spends})` : ""}`,
        )
        .join(" · "),
    });
  }

  const choosing = question.mode === "system" && !system;
  const task: BoardTaskView = choosing
    ? {
        target: "system",
        kind: "pick",
        interactive: true,
        values: Object.fromEntries(question.systems.map((id) => [id, 0])),
        chosen: {},
        verb: "Choose",
        unit: "system",
      }
    : {
        target: "planet",
        kind: "pick",
        interactive: true,
        values: Object.fromEntries(
          question.planets.map((planet) => [planet.id, planet.resources ?? 0]),
        ),
        chosen: Object.fromEntries(planets.map((id) => [id, true])),
        verb: "Ready",
        unit: "resources",
      };
  const more = need - planets.length;
  // A secondary with nothing chosen is a pass.
  const passing = question.mode === "follow" && !planets.length;
  const problem = choosing
    ? "Choose a system on the map."
    : more > 0 && !passing
      ? `Choose ${plural(more, "more planet")} on the map.`
      : null;
  const readied = planets.map((id) => planetOf(id).name).join(", ");
  const note =
    problem ??
    (passing
      ? "Nothing chosen: you pass."
      : [
          ...(system ? [systemLabel(tiles, system)] : []),
          planets.length ? `Ready ${readied}` : "No planet to ready",
        ].join(" · "));
  const answers = problem
    ? null
    : question.mode === "follow"
      ? passing
        ? ["no"]
        : ["yes", ...planets]
      : [...(system ? [system] : []), ...planets];
  return {
    blocks,
    task,
    note,
    problem,
    label:
      question.mode === "system"
        ? "Resolve primary"
        : question.mode === "follow"
          ? passing
            ? "Pass"
            : `Follow · ready ${plural(planets.length, "planet")}`
          : `Ready ${plural(planets.length, "planet")}`,
    changed: !!system || planets.length > 0,
    answers,
  };
}

/** A system of the map is chosen: it is the system of the question, or nothing changes. */
export function chooseReadySystem(
  choice: Choice,
  draft: ReadyDraft | null,
  system: string,
): ReadyDraft | null {
  const question = questionOf(choice);
  if (question?.mode !== "system" || !question.systems.includes(system)) {
    return null;
  }
  const staging = draft ?? NO_READY;
  return { ...staging, system: staging.system === system ? null : system };
}

/** A planet of the map is chosen, or given back. No more than the game readies are chosen. */
export function toggleReadyPlanet(
  choice: Choice,
  draft: ReadyDraft | null,
  planet: string,
): ReadyDraft | null {
  const question = questionOf(choice);
  if (!question?.planets.some((item) => item.id === planet)) {
    return null;
  }
  const staging = draft ?? NO_READY;
  const { system, planets, need } = stagedOf(question, staging);
  if (question.mode === "system" && !system) {
    return null;
  }
  if (planets.includes(planet)) {
    return { ...staging, planets: planets.filter((id) => id !== planet) };
  }
  return planets.length < need ? { ...staging, planets: [...planets, planet] } : null;
}

/** "Change" of a part: `system`, or `planet:<id>`. */
export function clearReadyPart(
  choice: Choice,
  draft: ReadyDraft | null,
  part: string,
): ReadyDraft | null {
  if (!questionOf(choice) || !draft) {
    return null;
  }
  if (part === "system") {
    return { ...draft, system: null };
  }
  const planet = part.startsWith("planet:") ? part.slice(7) : null;
  return planet ? { ...draft, planets: draft.planets.filter((id) => id !== planet) } : null;
}

/** Esc clears the last choice: the last planet, then the system. */
export function clearLastReady(choice: Choice, draft: ReadyDraft | null): ReadyDraft | null {
  const question = questionOf(choice);
  if (!question || !draft) {
    return null;
  }
  const { system, planets } = stagedOf(question, draft);
  return planets.length
    ? { system, planets: planets.slice(0, -1) }
    : system
      ? { system: null, planets: [] }
      : null;
}

/** The readying of planets as an editor (`editors.ts`). */
export const readyEditor: Editor<ReadyDraft> = {
  view: (update, choice, draft, printed) => answered(selectReady(update, choice, draft, printed)),
  system: chooseReadySystem,
  planet: toggleReadyPlanet,
  part: clearReadyPart,
  last: clearLastReady,
};
