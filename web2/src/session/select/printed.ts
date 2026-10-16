// What a component of the game is, as printed: the card of an option of the turn menu, and the
// card, ability or strategy card that asks a question (`DecisionContext.source`). The engine says
// which one; the catalogue has its name and its text. No rule text is written here.
import type { Catalog, ChoiceOption, DecisionSource, SessionUpdate } from "../wire";
import { plain } from "./components";
import { humanizeId, sentence, strategyCard } from "./names";

/** A printed component: the `source` block of a decision, the row of a list of cards. */
export interface Printed {
  name: string;
  /** "Action card", "Agent · The King’s Shield", "Relic". */
  type: string;
  text: string;
}

const LEADER = "component|leader|";
const TECH = "component|tech|";
const RELIC = "relic|";
const FACTION = "faction|";
const LEGENDARY = "legendary|";
const EXPLORATION = "play_area:";
export const ACTION_CARD = "action_card|";

/**
 * The printed primary and secondary of a strategy card of the set in play, as the catalogue
 * prints them, for the "?" of a heading or a row. Empty for a card that the catalogue does not
 * have; a part with no text is left out.
 */
export function strategyCardHelp(
  card: string,
  catalog: Catalog | null,
): { title: string; text: string }[] {
  const record = catalog?.records.strategy_cards.find(
    (item) => item.id === card || item.name === card,
  );
  return record
    ? [
        { title: `${record.name} · Primary`, text: record.primary_text },
        { title: `${record.name} · Secondary`, text: record.secondary_text },
      ].filter((item) => item.text)
    : [];
}

/** The id of the action card of the viewer that an option of the turn menu plays. */
export function heldCardOf(update: SessionUpdate, option: ChoiceOption): string | null {
  if (!option.id.startsWith(ACTION_CARD)) {
    return null;
  }
  const seat = update.viewer.role === "player" ? update.viewer.seat : null;
  const hand = update.view.players.find((player) => player.id === seat)?.held_action_cards ?? [];
  return hand[Number(option.id.slice(ACTION_CARD.length))] ?? null;
}

/** An action card by its id, the id that the engine names it by, or its printed name. */
function actionCard(key: string, catalog: Catalog | null): Printed | null {
  const card =
    catalog?.components.action_cards.find((item) => item.id === key || item.name === key) ??
    catalog?.records.action_cards.find((item) => item.automation_id === key || item.id === key);
  return card ? { name: card.name, type: "Action card", text: plain(card.text) } : null;
}

function leader(id: string, catalog: Catalog | null): Printed | null {
  const card = catalog?.components.leaders.find((item) => item.id === id);
  return card
    ? {
        name: card.name,
        type: [humanizeId(card.kind), card.title].filter(Boolean).join(" · "),
        text: plain(card.text),
      }
    : null;
}

/** A card of the catalogue that is a name and a text, of one kind. */
function named(
  cards: { id: string; name: string; text?: string }[] | undefined,
  id: string,
  type: string,
): Printed | null {
  const card = cards?.find((item) => item.id === id);
  return card ? { name: card.name, type, text: plain(card.text) } : null;
}

/**
 * The component that an option of the turn menu uses, as printed. Without the catalogue, or for
 * an option that names no card, it is the label of the engine.
 */
export function printedOption(
  update: SessionUpdate,
  option: ChoiceOption,
  catalog: Catalog | null,
): Printed {
  const id = option.id;
  const label = sentence((option.label || id).replace(/^play /, ""));
  const tail = (prefix: string) => id.slice(prefix.length);
  const card = heldCardOf(update, option);
  const found =
    card !== null
      ? actionCard(card, catalog)
      : id.startsWith(LEADER)
        ? leader(tail(LEADER), catalog)
        : id.startsWith(TECH)
          ? named(catalog?.components.technologies, tail(TECH), "Technology")
          : id.startsWith(RELIC)
            ? named(catalog?.components.relics, tail(RELIC), "Relic")
            : id.startsWith(FACTION)
              ? named(
                  catalog?.components.abilities,
                  tail(FACTION).split("|").at(-1) ?? "",
                  "Faction ability",
                )
              : id.startsWith(LEGENDARY)
                ? named(catalog?.components.legendary, tail(LEGENDARY), "Legendary planet")
                : id.startsWith(EXPLORATION)
                  ? named(catalog?.exploration.cards, tail(EXPLORATION), "Exploration card")
                  : null;
  return found ?? { name: label, type: card !== null ? "Action card" : "Component", text: "" };
}

/**
 * The card, ability or strategy card that asks a question, as printed. Null for a rule, which
 * the catalogue does not print, and for a source that it does not have.
 */
export function printedSource(
  source: DecisionSource | undefined,
  catalog: Catalog | null,
): Printed | null {
  if (!source) {
    return null;
  }
  if ("ActionCard" in source) {
    return actionCard(source.ActionCard, catalog);
  }
  if ("FactionAbility" in source) {
    return named(catalog?.components.abilities, source.FactionAbility, "Faction ability");
  }
  if ("StrategyCard" in source) {
    const { card, secondary } = source.StrategyCard;
    const record = catalog?.records.strategy_cards.find(
      (item) => item.id === card || item.name === card,
    );
    const name = record?.name ?? strategyCard(card, catalog).name;
    return {
      name,
      type: secondary ? "Strategy card · secondary" : "Strategy card · primary",
      text: (secondary ? record?.secondary_text : record?.primary_text) ?? "",
    };
  }
  if ("Agenda" in source) {
    return named(catalog?.components.agendas, source.Agenda, "Agenda");
  }
  if ("Content" in source) {
    const id = source.Content;
    return (
      leader(id, catalog) ??
      named(catalog?.components.relics, id, "Relic") ??
      named(catalog?.components.technologies, id, "Technology") ??
      named(catalog?.components.legendary, id, "Legendary planet") ??
      named(catalog?.exploration.cards, id, "Exploration card")
    );
  }
  return null;
}
