// A reaction window: what opened it, in game values, and what the viewer can use now, as printed.
// The engine says both (`reaction` of the update); this file words the trigger and the rows.
// No rule is decided here, and no rule text is written here.
import type { MenuRowView, OfferMode, OfferView } from "../../model";
import type {
  Catalog,
  Choice,
  DecisionTrigger,
  ReactionModes,
  ReactionOffer,
  SessionUpdate,
} from "../wire";
import { plain, timing } from "./components";
import { factionName, strategyCard, humanizeId } from "./names";

const KEYS = "123456789";

/** What an offer is, in the word of its card. */
const KIND: Record<ReactionOffer["kind"], string> = {
  leader: "Leader",
  ability: "Faction ability",
  technology: "Technology",
  promissory: "Promissory note",
  unit: "Unit",
  breakthrough: "Breakthrough",
  action_card: "Action card",
};

/** The mode that the viewer set for a reaction. A key that is not set is offered. */
export const modeOf = (modes: ReactionModes, key: string): OfferMode => modes[key] ?? "always";

/** The event of the engine in words: "TACTICAL_ACTION_ENDED" is "tactical action ended". */
const words = (event: string) => event.toLowerCase().replaceAll("_", " ");

/**
 * What opened the window: "After Hacan’s tactical action in system 18". An event that has no
 * wording of its own is said in its words, with the seat and the system that the engine names.
 */
export function triggerText(
  trigger: DecisionTrigger | undefined,
  update: SessionUpdate,
  catalog: Catalog | null,
): string {
  if (!trigger) {
    return "Reaction";
  }
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const faction = (seat: string) =>
    factionName(update.view.players.find((player) => player.id === seat)?.faction ?? seat, catalog);
  const { actor, system } = trigger;
  const mine = !!actor && actor === viewer;
  // "Hacan activates", "you activate"; "Hacan’s", "your".
  const who = !actor ? null : mine ? "you" : faction(actor);
  const whose = !actor ? null : mine ? "your" : `${faction(actor)}’s`;
  const s = mine ? "" : "s";
  const at = system ? ` in system ${system}` : "";
  const card = () => {
    const id = trigger.card ?? "";
    const printed = catalog?.components.action_cards.find((item) => item.id === id);
    return printed?.name ?? humanizeId(id);
  };
  const said = (): string | null => {
    switch (trigger.event_type) {
      case "TACTICAL_ACTION_ENDED":
        return whose && `${whose} tactical action${at}`;
      case "SYSTEM_ACTIVATED":
        return who && system ? `${who} activate${s} system ${system}` : null;
      case "ACTION_CARD_PLAYED":
        return who && trigger.card ? `${who} play${s} ${card()}` : null;
      case "ACTION_CARD_DISCARDED":
        return who && trigger.card ? `${who} discard${s} ${card()}` : null;
      case "STRATEGIC_ACTION_BEGAN":
        return whose && trigger.card
          ? `${whose} strategic action · ${strategyCard(trigger.card, catalog).name}`
          : null;
      case "TURN_BEGAN":
        return whose && `the start of ${whose} turn`;
      case "PLAYER_PASSED":
      case "TURN_PASSED":
        return who && `${who} pass${mine ? "" : "es"}`;
      case "MOVEMENT_FINISHED":
      case "SHIP_MOVED":
        return whose && `${whose} movement${system ? ` to system ${system}` : ""}`;
      case "AGENDA_REVEALED":
        return "an agenda is revealed";
      default:
        return null;
    }
  };
  const relation = trigger.relation === "when" ? "When" : "After";
  const text = said();
  if (text) {
    // "After the start of …" reads as the printed windows do: "At the start of …".
    return text.startsWith("the start of") ? `At ${text}` : `${relation} ${text}`;
  }
  const seat = actor ? ` · ${mine ? "You" : faction(actor)}` : "";
  return `${relation} ${words(trigger.event_type)}${seat}${system ? ` · system ${system}` : ""}`;
}

/** The reaction window of the viewer, as the action panel shows it. */
export interface ReactionWindow {
  heading: string;
  rows: MenuRowView[];
  /** The option that passes. Null: the window must be answered. */
  pass: string | null;
}

/**
 * The window of `choice`, when it is a reaction window. One row for each thing that it offers,
 * with the printed text and the setting of the viewer; the decline is the main button, not a row.
 */
export function selectReaction(
  update: SessionUpdate,
  choice: Choice,
  staged: string | null,
  catalog: Catalog | null,
  modes: ReactionModes,
): ReactionWindow | null {
  const facts = update.reaction;
  if (!facts) {
    return null;
  }
  const offered = new Set(facts.offers.map((offer) => offer.option));
  const offer = (item: ReactionOffer): OfferView => ({
    key: item.key,
    name: item.name,
    mode: modeOf(modes, item.key),
    ownTurn: facts.own_turn_applies,
  });
  const rows: Omit<MenuRowView, "key">[] = [
    ...facts.offers.map((item) => ({
      title: item.name,
      state: item.kind === "leader" && item.role ? humanizeId(item.role) : KIND[item.kind],
      text: [timing(item.window), plain(item.text)].filter(Boolean).join(" "),
      selected: item.option === staged,
      offer: offer(item),
      disabled: false,
      intent: { type: "chooseOption" as const, option: item.option },
    })),
    // What the window asks beside its cards: a rule that has no card.
    ...choice.options
      .filter((option) => option.kind !== "decline" && !offered.has(option.id))
      .map((option) => ({
        title: option.label || option.id,
        selected: option.id === staged,
        disabled: false,
        intent: { type: "chooseOption" as const, option: option.id },
      })),
  ];
  return {
    heading: triggerText(facts.trigger, update, catalog),
    rows: rows.map((row, index) => ({ key: KEYS[index] ?? "", ...row })),
    pass: choice.options.find((option) => option.kind === "decline")?.id ?? null,
  };
}
