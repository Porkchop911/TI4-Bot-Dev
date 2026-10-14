// The agendas: the laws in play with the outcome that passed, and the rest of the deck agenda by
// agenda. The engine says which laws are in play and how many cards the deck holds; the catalogue
// has the printed agendas and the deck that a game begins with. Every agenda is in the deck once.
import type { AgendasView, AgendaView, LawView } from "../../model";
import type { Catalog, SessionUpdate } from "../wire";
import { plain } from "./components";
import { titleCase } from "./names";

/** The group of an agenda that the catalogue does not have. */
const OTHER = "Other";

export function selectAgendas(update: SessionUpdate, catalog: Catalog | null): AgendasView {
  const deck = catalog?.components.agendas ?? [];
  const printed = new Map(deck.map((card) => [card.id, card]));
  const kind = (id: string) => {
    const found = printed.get(id);
    return found ? [found.kind, found.elect].filter(Boolean).join(" · ") : "";
  };
  const laws = update.view.table.laws;
  const seats = new Set(update.view.players.map((player) => player.id));
  const secrets = new Map(
    (catalog?.components.secret_objectives ?? []).map((card) => [card.id, card.name]),
  );
  // What passed: a seat, "for" or "against", or the id of what was elected.
  const law = (id: string): LawView => {
    const found = printed.get(id);
    const passed = laws[id] ?? "";
    return {
      name: found?.name ?? titleCase(id),
      kind: kind(id),
      text: plain(passed === "against" ? found?.against : found?.text),
      outcome: seats.has(passed)
        ? { seat: passed }
        : { text: secrets.get(passed) ?? titleCase(passed) },
    };
  };
  const row = (id: string): AgendaView & { group: string } => {
    const found = printed.get(id);
    return {
      name: found?.name ?? titleCase(id),
      kind: kind(id),
      text: plain([found?.text, found?.against].filter(Boolean).join("\n")),
      group: found?.group ?? OTHER,
    };
  };
  const byName = (a: { name: string }, b: { name: string }) => a.name.localeCompare(b.name);
  const rest = deck
    .filter((card) => !(card.id in laws))
    .map((card) => row(card.id))
    .sort(byName);
  // The groups in the order of the catalogue. A group that it does not list is after them.
  const groups = [
    ...new Set([...(catalog?.components.agenda_groups ?? []), ...rest.map((card) => card.group)]),
  ];
  return {
    laws: Object.keys(laws).map(law).sort(byName),
    rest: rest.length,
    deck: update.agendas?.deck ?? null,
    reference: groups
      .map((group) => ({
        group,
        cards: rest
          .filter((card) => card.group === group)
          .map(({ name, kind, text }) => ({ name, kind, text })),
      }))
      .filter((group) => group.cards.length > 0),
  };
}
