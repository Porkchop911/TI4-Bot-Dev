// The exploration: what it says about a planet, and the decks. The engine says which planets are
// explored and what the decks hold; the catalogue has the printed names and texts.
import type {
  AttachmentView,
  ExplorationCardView,
  ExplorationDeck,
  ExplorationDeckView,
  PlanetTraitsView,
  PlanetType,
} from "../../model";
import type { Catalog, ExplorationCatalog, PlanetMetaView, SessionUpdate } from "../wire";
import { TECH_COLORS, factionName, humanizeId } from "./names";

/** The decks in the order of every screen, with the key of the engine. */
export const DECKS: [key: string, deck: ExplorationDeck][] = [
  ["CULTURAL", "cultural"],
  ["HAZARDOUS", "hazardous"],
  ["INDUSTRIAL", "industrial"],
  ["FRONTIER", "frontier"],
];
const TYPES: Record<string, PlanetType> = {
  CULTURAL: "cultural",
  HAZARDOUS: "hazardous",
  INDUSTRIAL: "industrial",
};
const SKIPS = { B: "Blue", G: "Green", Y: "Yellow", R: "Red" };

const signed = (value: number) => (value < 0 ? `−${-value}` : `+${value}`);

function attachment(id: string, catalog: ExplorationCatalog | null): AttachmentView {
  const card = catalog?.attachments.find((item) => item.id === id);
  if (!card) {
    return { name: humanizeId(id), effect: "", text: "" };
  }
  const skips = (card.tech_specialties ?? []).flatMap((name) => {
    const color = TECH_COLORS[name];
    return color ? [`${SKIPS[color]} skip`] : [];
  });
  return {
    name: card.name,
    effect: [
      ...(card.resources || card.influence
        ? [`${signed(card.resources)} / ${signed(card.influence)}`]
        : []),
      ...skips,
    ].join(" · "),
    text: card.text ?? "",
  };
}

/** A planet as printed, before a game: its type, and nothing that the exploration changed. */
export const printedTraits = (planet: PlanetMetaView): PlanetTraitsView => ({
  types: (planet.traits ?? []).flatMap((name) => {
    const type = TYPES[name.toUpperCase()];
    return type ? [type] : [];
  }),
  unexplored: false,
  attachments: [],
});

/** What the exploration says about the planets of one update. */
export function planetTraits(update: SessionUpdate, catalog: ExplorationCatalog | null) {
  const attached = new Map<string, string[]>();
  for (const system of Object.values(update.view.board.systems)) {
    for (const planet of Object.values(system.planets)) {
      attached.set(planet.planet_id, planet.attachments ?? []);
    }
  }
  const explores = update.exploration?.explores ?? {};
  return (planet: PlanetMetaView): PlanetTraitsView => ({
    // The decks of the engine where it names them: an attachment can give a planet a type.
    types: (explores[planet.id] ?? planet.traits ?? []).flatMap((name) => {
      const type = TYPES[name.toUpperCase()];
      return type ? [type] : [];
    }),
    unexplored: planet.id in explores,
    attachments: (attached.get(planet.id) ?? []).map((id) => attachment(id, catalog)),
  });
}

/**
 * The exploration card that asks the open choice, as printed: the card that the seat of the
 * choice drew last, while the game has not said what came of it.
 */
export function exploringCard(
  update: SessionUpdate,
  catalog: ExplorationCatalog | null,
): { name: string; type: string; text: string } | null {
  const record = update.exploration?.drawn.at(-1);
  if (record?.outcome !== "" || record.player !== update.pending_choice?.choice.player) {
    return null;
  }
  const card = catalog?.cards.find((item) => item.id === record.card);
  const planet = (update.view.board.map_tiles ?? [])
    .flatMap((tile) => tile.planets ?? [])
    .find((item) => item.id === record.planet);
  return {
    name: card?.name ?? humanizeId(record.card),
    type: [
      `${humanizeId(record.deck.toLowerCase())} exploration`,
      ...(planet ? [planet.label] : []),
    ].join(" · "),
    text: card?.text ?? "",
  };
}

/** No type, explored, nothing attached: a planet of a screen that has no facts for it. */
export const NO_TRAITS: PlanetTraitsView = { types: [], unexplored: false, attachments: [] };

/** The four decks: what each still holds, card by card, and who has its relic fragments. */
export function selectExploration(
  update: SessionUpdate,
  catalog: ExplorationCatalog | null,
  /** The printed names. Without it a faction is shown by its id. */
  names: Catalog | null = null,
): ExplorationDeckView[] {
  const facts = update.exploration;
  if (!facts) {
    return [];
  }
  const printed = new Map((catalog?.cards ?? []).map((card) => [card.id, card]));
  const planets = new Map(
    (update.view.board.map_tiles ?? []).flatMap((tile) =>
      (tile.planets ?? []).map((planet) => [planet.id, planet.label] as const),
    ),
  );
  const faction = (seat: string) =>
    factionName(update.view.players.find((player) => player.id === seat)?.faction ?? seat, names);
  return DECKS.map(([key, deck]) => {
    const left = facts.decks[key] ?? [];
    const drawn = facts.drawn.filter((record) => record.deck === key);
    // Without the catalogue the deck is what is left of it and what was drawn.
    const all = catalog?.decks[key] ?? [...left, ...drawn.map((record) => record.card)];
    const rows = new Map<string, ExplorationCardView>();
    for (const id of all) {
      const card = printed.get(id);
      const name = card?.name ?? humanizeId(id);
      const row = rows.get(name) ?? {
        name,
        kind: card?.resolution ?? "",
        text: card?.text ?? "",
        left: 0,
        total: 0,
        drawn: [],
      };
      row.total++;
      rows.set(name, row);
    }
    for (const id of left) {
      const row = rows.get(printed.get(id)?.name ?? humanizeId(id));
      if (row) {
        row.left++;
      }
    }
    for (const record of drawn) {
      rows
        .get(printed.get(record.card)?.name ?? humanizeId(record.card))
        ?.drawn.push(
          [
            `Round ${record.round}`,
            faction(record.player),
            ...(record.planet ? [planets.get(record.planet) ?? record.planet] : []),
          ].join(" · "),
        );
    }
    return {
      deck,
      title: humanizeId(deck),
      left: left.length,
      total: all.length,
      fragments: update.view.players.flatMap((player) => {
        const count = player.relic_fragments?.[key] ?? 0;
        return count > 0 ? [{ seat: player.id, count }] : [];
      }),
      cards: [...rows.values()].sort((a, b) => a.name.localeCompare(b.name)),
    };
  });
}

/** An exploration of the events, as Live showed its card, and what came of it. */
export interface ExploredView {
  /** The printed name of the card. */
  name: string;
  /** "Cultural exploration · Primor", as the card of an exploration that asks says it. */
  type: string;
  /** The printed text. */
  text: string;
  /** What came of it, in game values: "Attached Demilitarized Zone", "Gained 1 trade good". */
  effect: string;
}

/**
 * The exploration of a fact `explored` of the events: the card as printed, and what came of it,
 * from the record of the engine (`ExplorationFacts.drawn`, every card drawn, public). A card
 * leaves its deck when it is drawn, so its id finds its record.
 */
export function exploredCard(
  update: SessionUpdate,
  catalog: ExplorationCatalog | null,
  fact: { seat: string; card: string; deck: string; planet?: string; system?: string },
): ExploredView {
  const card = catalog?.cards.find((item) => item.id === fact.card);
  const record = update.exploration?.drawn.findLast(
    (item) => item.card === fact.card && item.player === fact.seat,
  );
  const tiles = update.view.board.map_tiles ?? [];
  const planetName = (id: string) =>
    tiles.flatMap((tile) => tile.planets ?? []).find((item) => item.id === id)?.label ??
    humanizeId(id);
  const systemName = (id: string) => {
    const label = tiles.find((tile) => tile.system_id === id)?.label;
    return label ? `${label} · #${id}` : `#${id}`;
  };
  const place = fact.planet
    ? planetName(fact.planet)
    : fact.system
      ? systemName(fact.system)
      : null;
  const parts: string[] = [];
  const attached = record?.outcome.startsWith("attached:") ? record.outcome.slice(9) : "";
  if (attached) {
    parts.push(`attached ${attachment(attached, catalog).name}`);
  }
  for (const tag of record?.placed ?? []) {
    const [kind, first, second] = tag.split(":");
    if (kind === "wormhole") {
      parts.push(`placed a ${first.toLowerCase()} wormhole in ${systemName(second)}`);
    } else if (kind === "ion_storm") {
      parts.push(`placed the ion storm in ${systemName(first)}`);
    } else if (kind === "planet") {
      parts.push(`placed ${planetName(first)} in ${systemName(second)}`);
    }
  }
  // The words of the engine; the rules of a card in the play area are its printed text.
  const result = (record?.result ?? "")
    .replace(/\s*\(ACTION:[^)]*\)/, "")
    .replace(/\ba (?=[aeiou])/g, "an ");
  if (result) {
    parts.push(result);
  }
  // A card that the seat resolved with nothing to show for it (a sacrifice it declined).
  const effect = parts.join(", ") || (record?.outcome === "resolved" ? "no effect" : "");
  return {
    name: card?.name ?? humanizeId(fact.card),
    type: [`${humanizeId(fact.deck.toLowerCase())} exploration`, ...(place ? [place] : [])].join(
      " · ",
    ),
    text: card?.text ?? "",
    effect: effect ? effect[0].toUpperCase() + effect.slice(1) : "",
  };
}
