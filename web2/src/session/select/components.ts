// The cards in the play area of a player: the faction abilities, the leaders, and the cards that
// the player uses. The engine says what a player has and what is exhausted; the catalogue has the
// printed names and texts, and says which cards are used.
import type { ComponentCardView, OfferView, PlayerRowView } from "../../model";
import type { Catalog, PlayerView, PrintedCard, ReactionModes, SessionUpdate } from "../wire";
import { nameOfKey, humanizeId } from "./names";

const KINDS = ["agent", "commander", "hero"];
const LEADER_STATES: Record<string, ComponentCardView["state"]> = {
  locked: "locked",
  readied: "ready",
  unlocked: "ready",
  exhausted: "exhausted",
  purged: "purged",
};

/** "ACTION:", "At the start of a ground combat round:": the timing as the cards print it. */
export const timing = (window: string | undefined) =>
  !window || /[:.]$/.test(window) ? (window ?? "") : `${window}:`;
/** The corpus marks keywords for another renderer: "**STAR FORGE**". */
export const plain = (text: string | undefined) => (text ?? "").replaceAll("*", "");

const byId = <T extends { id: string }>(cards: T[] | undefined) =>
  new Map((cards ?? []).map((card) => [card.id, card]));

/** A card whose printed text begins with "ACTION:" is a component action: no window offers it. */
const isAction = (text: string | undefined) => /^\s*action\b/i.test(text ?? "");

export function componentCards(
  update: SessionUpdate,
  player: PlayerView,
  catalog: Catalog | null,
  /**
   * The modes that the viewer set for its reactions, for the row of the viewer: its cards that a
   * window can offer have the setting. Null for the row of another seat.
   */
  modes: ReactionModes | null = null,
): Pick<PlayerRowView, "abilities" | "leaders" | "components" | "muted"> {
  /** The keys of the cards of the sheet that have the setting. */
  const shown = new Set<string>();
  const offer = (key: string, name: string, offered: boolean): OfferView | null => {
    if (!modes || !offered) {
      return null;
    }
    shown.add(key);
    return { key, name, mode: modes[key] ?? "always", ownTurn: true };
  };
  const printed = catalog?.components;
  const abilities = byId(printed?.abilities);
  const leaders = byId(printed?.leaders);
  // Without the catalogue the kind of a leader is the end of its id: "solagent".
  const kindOf = (id: string) =>
    leaders.get(id)?.kind ?? KINDS.find((kind) => id.endsWith(kind)) ?? "";

  /** A card that is used is ready or exhausted. A card that always holds has no state. */
  const used = (
    card: PrintedCard | undefined,
    id: string,
    kind: string,
    exhausted: string[] | undefined,
  ): ComponentCardView => ({
    name: card?.name ?? humanizeId(id),
    kind,
    window: "",
    text: plain(card?.text),
    state: card && !card.used ? null : exhausted?.includes(id) ? "exhausted" : "ready",
    unlock: "",
    offer:
      kind === "Technology"
        ? offer(`technology:${id}`, card?.name ?? humanizeId(id), !!card && !isAction(card.text))
        : null,
  });

  const technologies = byId(printed?.technologies);
  const relics = byId(printed?.relics);
  const legendary = byId(printed?.legendary);
  const kept = byId(catalog?.exploration.cards);
  const labels = new Map(
    (update.view.board.map_tiles ?? []).flatMap((tile) =>
      (tile.planets ?? []).map((planet) => [planet.id, planet] as const),
    ),
  );
  const planets = Object.values(update.view.board.systems)
    .flatMap((system) => Object.values(system.planets))
    .filter((planet) => planet.controlled_by === player.id)
    .map((planet) => planet.planet_id)
    .filter((id) => labels.get(id)?.legendary);

  const cards = {
    abilities: (printed?.factions[player.faction] ?? []).map((id) => {
      const card = abilities.get(id);
      return {
        name: card?.name ?? humanizeId(id),
        kind: "Faction ability",
        window: timing(card?.window),
        text: plain(card?.text),
        state: null,
        unlock: "",
        // An ability with a timing is used in a window.
        offer: offer(`ability:${id}`, card?.name ?? humanizeId(id), !!card?.window),
      };
    }),
    leaders: Object.entries(player.leaders)
      .sort(([a], [b]) => KINDS.indexOf(kindOf(a)) - KINDS.indexOf(kindOf(b)))
      .map(([id, status]) => {
        const card = leaders.get(id);
        const state = LEADER_STATES[status] ?? null;
        return {
          name: card?.name ?? humanizeId(kindOf(id) || id),
          kind: [humanizeId(kindOf(id)), card?.title].filter(Boolean).join(" · "),
          window: timing(card?.window),
          text: plain(card?.text),
          state,
          unlock: state === "locked" ? (card?.unlock ?? "") : "",
          offer: offer(
            `leader:${id}`,
            card?.name ?? humanizeId(id),
            !!card?.window && !isAction(card.window) && state !== "purged",
          ),
        };
      }),
    components: [
      // Without the catalogue no technology is known to be used: the sheet lists them all by id.
      ...player.technologies
        .filter((id) => technologies.get(id)?.used)
        .map((id) => used(technologies.get(id), id, "Technology", player.exhausted_technologies)),
      ...player.relics.map((id) => used(relics.get(id), id, "Relic", player.exhausted_relics)),
      ...(player.exploration_cards ?? []).map((id) => ({
        name: kept.get(id)?.name ?? humanizeId(id),
        kind: "Exploration card",
        window: "",
        text: plain(kept.get(id)?.text),
        state: "ready" as const,
        unlock: "",
      })),
      ...planets.map((id) =>
        used(
          legendary.get(id),
          id,
          `Legendary planet · ${labels.get(id)?.label ?? humanizeId(id)}`,
          player.exhausted_legendary,
        ),
      ),
    ],
  };
  return {
    ...cards,
    // What is muted and has no card above: the sheet is where it is set back.
    ...(modes
      ? {
          muted: Object.entries(modes)
            .filter(([key]) => key.includes(":") && !shown.has(key))
            .map(([key, mode]) => ({ key, name: nameOfKey(key, catalog), mode, ownTurn: true })),
        }
      : {}),
  };
}
