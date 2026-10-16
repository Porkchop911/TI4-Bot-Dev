// The placement of agenda cards on the top or the bottom of the deck (`details.agendas`, the
// engine's `AgendaPlacementHints`, with the options `top` and `bottom`): the game asks one agenda
// at a time, and the first question already names every agenda to place, in the order that they
// are asked. Here they are one screen: each agenda as printed with "Top of the deck" and "Bottom
// of the deck", and the deck after it as the game builds it. The main button answers every agenda
// at once, by the ids of their options.
import type { BlockView, MenuRowView, RowView } from "../../model";
import type { AgendaPlacementHints, Catalog, Choice } from "../wire";
import { plain } from "./components";
import { humanizeId } from "./names";
import { type Editor, answered, hintFieldsOf } from "./editor";

export type AgendaPlace = "top" | "bottom";

/** What the player staged: where each agenda goes, in the order that it was chosen. */
export interface AgendaDraft {
  placed: { agenda: string; place: AgendaPlace }[];
}

interface Agenda {
  id: string;
  name: string;
  type: string;
  text: string;
}

/** The agendas to place, as printed, or null for any other question. */
function agendasOf(choice: Choice, printed: Catalog | null): Agenda[] | null {
  const details = choice.details ?? {};
  const ids = new Set(choice.options.map((option) => option.id));
  if (!ids.has("top") || !ids.has("bottom") || ids.size !== 2) {
    return null;
  }
  const one = details.agenda as Record<string, unknown> | undefined;
  const listed =
    hintFieldsOf<AgendaPlacementHints>(choice, "agendas")?.agendas ??
    (typeof one?.id === "string" ? [one.id] : []);
  if (!listed.length) {
    return null;
  }
  const cards = new Map((printed?.components.agendas ?? []).map((card) => [card.id, card]));
  return listed.map((id) => {
    const card = cards.get(id);
    // The details print the agenda of this question; the catalogue prints every agenda.
    const own = one?.id === id ? one : null;
    return card
      ? {
          id,
          name: card.name,
          type: ["Agenda", card.kind, card.elect].filter(Boolean).join(" · "),
          text: plain([card.text, card.against].filter(Boolean).join("\n")),
        }
      : {
          id,
          name: typeof own?.name === "string" ? own.name : humanizeId(id),
          type: typeof own?.type === "string" && own.type ? `Agenda · ${own.type}` : "Agenda",
          text: plain(
            [own?.text1, own?.text2].filter((text) => typeof text === "string" && text).join("\n"),
          ),
        };
  });
}

export interface AgendaDeckView {
  blocks: BlockView[];
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** The answers, by the ids of their options, in the order that the game asks. Null: not yet. */
  answers: string[] | null;
}

const KEYS = "123456789";

/** The staged places of the agendas of the question. */
function placesOf(agendas: Agenda[], draft: AgendaDraft | null): Map<string, AgendaPlace> {
  const ids = new Set(agendas.map((agenda) => agenda.id));
  return new Map(
    (draft?.placed ?? [])
      .filter((item) => ids.has(item.agenda))
      .map((item) => [item.agenda, item.place]),
  );
}

/** The placement of agenda cards. Null for any other question. */
export function selectAgendaDeck(
  choice: Choice,
  draft: AgendaDraft | null,
  printed: Catalog | null = null,
): AgendaDeckView | null {
  const agendas = agendasOf(choice, printed);
  if (!agendas) {
    return null;
  }
  const places = placesOf(agendas, draft);
  const blocks: BlockView[] = agendas.flatMap((agenda, index): BlockView[] => [
    { kind: "source", name: agenda.name, type: agenda.type, text: agenda.text },
    {
      kind: "menu",
      rows: (["top", "bottom"] as const).map((place, at): MenuRowView => ({
        key: KEYS[index * 2 + at] ?? "",
        title: place === "top" ? "Top of the deck" : "Bottom of the deck",
        selected: places.get(agenda.id) === place,
        disabled: false,
        intent: { type: "chooseOption", option: place, group: `agenda:${agenda.id}` },
      })),
    },
  ]);
  // The deck as the game builds it: an agenda on top goes over the ones placed on top before it,
  // an agenda on the bottom under the ones placed on the bottom before it.
  const tops: Agenda[] = [];
  const bottoms: Agenda[] = [];
  for (const agenda of agendas) {
    const place = places.get(agenda.id);
    if (place === "top") {
      tops.unshift(agenda);
    } else if (place === "bottom") {
      bottoms.push(agenda);
    }
  }
  const deck: RowView[] = [
    ...tops.map((agenda, at) => ({
      icon: "card" as const,
      title: [at ? "Second from the top" : "Top of the deck · the next agenda"],
      subtitle: agenda.name,
    })),
    ...bottoms.map((agenda, at) => ({
      icon: "card" as const,
      title: [at === bottoms.length - 1 ? "Bottom of the deck" : "Second from the bottom"],
      subtitle: agenda.name,
    })),
  ];
  blocks.push({
    kind: "card",
    title: "Agenda deck after",
    aside: `${places.size} / ${agendas.length} placed`,
    rows: deck.length ? deck : [{ icon: "card", title: ["No card placed"] }],
  });
  const left = agendas.length - places.size;
  const problem = left
    ? left === agendas.length
      ? `Place ${agendas.length === 1 ? "the agenda card" : `the ${agendas.length} agenda cards`}.`
      : `Place ${left === 1 ? "1 more agenda card" : `${left} more agenda cards`}.`
    : null;
  return {
    blocks,
    note:
      problem ??
      agendas
        .map(
          (agenda) =>
            `${agenda.name} ${places.get(agenda.id) === "top" ? "on top" : "on the bottom"}`,
        )
        .join(" · "),
    problem,
    label: agendas.length === 1 ? "Place the agenda card" : `Place ${agendas.length} agenda cards`,
    changed: places.size > 0,
    answers: problem ? null : agendas.map((agenda) => places.get(agenda.id)!),
  };
}

/** A place of an agenda is chosen, or taken back when it was chosen. */
export function placeAgenda(
  choice: Choice,
  draft: AgendaDraft | null,
  group: string | undefined,
  place: string,
): AgendaDraft | null {
  const agenda = group?.startsWith("agenda:") ? group.slice(7) : null;
  const agendas = agendasOf(choice, null);
  if (!agenda || !agendas?.some((item) => item.id === agenda)) {
    return null;
  }
  if (place !== "top" && place !== "bottom") {
    return null;
  }
  const placed = (draft?.placed ?? []).filter((item) => item.agenda !== agenda);
  const before = draft?.placed.find((item) => item.agenda === agenda);
  return { placed: before?.place === place ? placed : [...placed, { agenda, place }] };
}

/** Esc clears the last choice. */
export function clearLastAgenda(draft: AgendaDraft | null): AgendaDraft | null {
  return draft?.placed.length ? { placed: draft.placed.slice(0, -1) } : null;
}

/** The placement of agenda cards as an editor (`editors.ts`). */
export const agendaDeckEditor: Editor<AgendaDraft> = {
  view: (_, choice, draft, printed) => answered(selectAgendaDeck(choice, draft, printed)),
  option: placeAgenda,
  last: (_, draft) => clearLastAgenda(draft),
};
