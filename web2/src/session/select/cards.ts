// The action cards: the hand of the viewer as printed, and the deck card by card. The engine says
// what the viewer holds, what the discard pile has and what the hand limit is; the catalogue has
// the printed cards and the deck that a game begins with. Copies of a card have ids of their own
// and one name: a card here is a printed name.
import type { ActionCardView, ActionCardsView } from "../../model";
import type { Catalog, ReactionModes, SessionUpdate } from "../wire";
import { plain, timing } from "./components";
import { humanizeId } from "./names";

/** What the player set for cards. Only `modes` changes what the viewer is asked. */
export interface CardSettings {
  /**
   * The mode of each reaction that the viewer set: an action card by its printed name, the card
   * of an ability by its key. Where the game runs has them (`Transport.setReactionMode`), and
   * applies them (`ti4_host::view::reaction_modes`).
   */
  modes: ReactionModes;
  /** The printed names of the cards that the viewer acts as if holding. Not wired to anything yet. */
  faked: string[];
}

export const NO_CARD_SETTINGS: CardSettings = { modes: {}, faked: [] };

/** The group of a card that the catalogue does not have. */
const OTHER = "Other";

/** A card with the window "Action" is a component action: no window ever offers it. */
const isAction = (window: string) => window.trim().toLowerCase() === "action";

export function selectActionCards(
  update: SessionUpdate,
  catalog: Catalog | null,
  settings: CardSettings,
): ActionCardsView | null {
  const viewer = update.viewer;
  const me =
    viewer.role === "player"
      ? update.view.players.find((player) => player.id === viewer.seat)
      : null;
  if (!me) {
    return null;
  }
  const deck = catalog?.components.action_cards ?? [];
  const printed = new Map(deck.map((card) => [card.id, card]));
  const nameOf = (id: string) => printed.get(id)?.name ?? humanizeId(id);
  const count = (ids: string[]) => {
    const counts = new Map<string, number>();
    for (const id of ids) {
      counts.set(nameOf(id), (counts.get(nameOf(id)) ?? 0) + 1);
    }
    return counts;
  };
  const facts = update.action_cards;
  const held = count(me.held_action_cards ?? []);
  // A card that is held is not faked, and a component action has nothing to fake.
  const canFake = (name: string) => !held.has(name) && !card(name).action;
  const gone = [...(facts?.discard ?? []), ...(facts?.salvage ?? [])];
  const discarded = count(gone);
  const total = count(deck.map((card) => card.id));
  // The first copy of a name has its text: the copies are printed the same.
  const byName = new Map<string, ActionCardView>();
  for (const card of deck) {
    if (!byName.has(card.name)) {
      const action = isAction(card.window);
      byName.set(card.name, {
        name: card.name,
        window: action ? "ACTION:" : timing(card.window),
        text: plain(card.text),
        action,
        group: card.group,
      });
    }
  }
  const card = (name: string): ActionCardView =>
    byName.get(name) ?? { name, window: "", text: "", action: false, group: OTHER };
  const names = [...new Set([...byName.keys(), ...held.keys(), ...discarded.keys()])].sort();
  // The groups in the order of the catalogue. A group that it does not list is after them.
  const groups = [
    ...new Set([
      ...(catalog?.components.action_card_groups ?? []),
      ...names.map((name) => card(name).group),
    ]),
  ];
  const byGroup = (a: string, b: string) =>
    groups.indexOf(card(a).group) - groups.indexOf(card(b).group) || a.localeCompare(b);
  return {
    held: me.action_cards_count,
    limit: facts?.hand_limit ?? null,
    deck: facts ? { left: facts.deck, total: deck.length, discarded: gone.length } : null,
    hand: [...held.keys()].sort(byGroup).map((name) => ({
      ...card(name),
      copies: held.get(name) ?? 0,
      offer: card(name).action
        ? null
        : { key: name, name, mode: settings.modes[name] ?? "always", ownTurn: true },
    })),
    faked: settings.faked.filter(canFake).map(card),
    reference: groups
      .map((group) => ({
        group,
        cards: names
          .filter((name) => card(name).group === group)
          .map((name) => {
            const all = total.get(name) ?? 0;
            const mine = held.get(name) ?? 0;
            const out = discarded.get(name) ?? 0;
            return {
              ...card(name),
              total: all,
              discarded: out,
              held: mine,
              open: Math.max(0, all - out - mine),
              faked: canFake(name) && settings.faked.includes(name),
            };
          }),
      }))
      .filter((group) => group.cards.length > 0),
  };
}
