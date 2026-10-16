// The secret objectives: those of the viewer as printed, and the deck objective by objective. The
// engine says what the viewer holds and what each seat has scored; the catalogue has the printed
// objectives and the deck that a game begins with. Every objective is in the deck once.
import type { DeckSecretView, SecretObjectiveView, SecretsView } from "../../model";
import type { Catalog, SessionUpdate } from "../wire";
import { plain } from "./components";
import { humanizeId } from "./names";

/** The group of an objective that the catalogue does not have. */
const OTHER = "Other";

export function selectSecrets(update: SessionUpdate, catalog: Catalog | null): SecretsView | null {
  const viewer = update.viewer;
  const me =
    viewer.role === "player"
      ? update.view.players.find((player) => player.id === viewer.seat)
      : null;
  if (!me) {
    return null;
  }
  const deck = catalog?.components.secret_objectives ?? [];
  const printed = new Map(deck.map((card) => [card.id, card]));
  const card = (id: string): SecretObjectiveView => {
    const found = printed.get(id);
    return found
      ? {
          name: found.name,
          phase: `${found.phase} phase`,
          text: plain(found.text),
          group: found.group,
        }
      : { name: humanizeId(id), phase: "", text: "", group: OTHER };
  };
  const facts = update.secret_objectives;
  // Where an objective is: scored is public; a hand is the viewer's, or one that a law reveals.
  const where = new Map<string, Pick<DeckSecretView, "state" | "by">>();
  for (const player of update.view.players) {
    for (const id of player.held_secret_objectives ?? []) {
      where.set(id, { state: "held", by: player.id });
    }
  }
  for (const [seat, ids] of Object.entries(facts?.scored ?? {})) {
    for (const id of ids) {
      where.set(id, { state: "scored", by: seat });
    }
  }
  const scored = facts?.scored[me.id] ?? [];
  const held = (me.held_secret_objectives ?? []).filter((id) => !scored.includes(id));
  const byName = (a: string, b: string) => card(a).name.localeCompare(card(b).name);
  const ids = [...new Set([...printed.keys(), ...where.keys()])].sort(byName);
  // The groups in the order of the catalogue. A group that it does not list is after them.
  const groups = [
    ...new Set([
      ...(catalog?.components.secret_objective_groups ?? []),
      ...ids.map((id) => card(id).group),
    ]),
  ];
  const row = (id: string): DeckSecretView => {
    const at = where.get(id) ?? { state: "unseen" as const, by: null };
    return { ...card(id), ...at, mine: at.by === me.id };
  };
  const reference = groups
    .map((group) => {
      const cards = ids.filter((id) => card(id).group === group).map(row);
      return { group, unseen: cards.filter((item) => item.state === "unseen").length, cards };
    })
    .filter((group) => group.cards.length > 0);
  return {
    held: held.length,
    scored: scored.length,
    limit: facts?.hand_limit ?? null,
    mine: [
      ...[...held].sort(byName).map((id) => ({ ...card(id), scored: false })),
      ...[...scored].sort(byName).map((id) => ({ ...card(id), scored: true })),
    ],
    total: deck.length,
    unseen: reference.reduce((sum, group) => sum + group.unseen, 0),
    deck: facts?.deck ?? null,
    reference,
  };
}
