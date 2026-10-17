// A reaction window: what opened it, in game values, and what the viewer can use now, as printed.
// The engine says both (`reaction` of the update); this file words the trigger and the rows.
// No rule is decided here, and no rule text is written here.
import { previewOf, previewOfCard } from "./effects";
import { optionWords } from "./notes";
import type { BlockView, InterruptView, MenuRowView, OfferMode, OfferView } from "../../model";
import type {
  Catalog,
  Choice,
  DecisionTrigger,
  ReactionModes,
  ReactionOffer,
  SessionUpdate,
} from "../wire";
import { plain, timing } from "./components";
import { systemLabel, tilesOf } from "./movement";
import { humanizeId, phaseName, seatLabel, sentence, strategyCard } from "./names";
import { printedSource } from "./printed";

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
  // The seat as the page names it everywhere: its player, else its faction.
  const faction = (seat: string) => seatLabel(update, seat, catalog);
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
      case "AGENDA_REVEALED": {
        const agenda = trigger.agenda
          ? catalog?.components.agendas.find((item) => item.id === trigger.agenda)?.name
          : undefined;
        return agenda ? `${agenda} is revealed` : "an agenda is revealed";
      }
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
  /** What the main button says for the staged row: "Play Sabotage", "Use T'ro". Null: none is staged. */
  use: string | null;
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
  // What a row would do now, as the engine words it, with the printed text of the cards that it
  // names behind "?".
  const effect = (option: string, name?: string): Partial<MenuRowView> => {
    const preview = previewOf(choice, option, update, catalog, name);
    return preview
      ? { effect: preview.text, ...(preview.help.length ? { help: preview.help } : {}) }
      : {};
  };
  const rows: Omit<MenuRowView, "key">[] = [
    ...facts.offers.map((item) => ({
      title: item.name,
      state: item.kind === "leader" && item.role ? humanizeId(item.role) : KIND[item.kind],
      ...effect(item.option, item.name),
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
        title: optionWords(choice, option, update, catalog).text,
        ...effect(option.id),
        selected: option.id === staged,
        disabled: false,
        intent: { type: "chooseOption" as const, option: option.id },
      })),
  ];
  // A slot of several action cards is one option: the game asks which card after it.
  const using = facts.offers.filter((item) => item.option === staged);
  const use = !staged
    ? null
    : using.length === 1
      ? `${using[0].kind === "action_card" ? "Play" : "Use"} ${using[0].name}`
      : using.length
        ? "Play an action card"
        : "Use";
  return {
    heading: triggerText(facts.trigger, update, catalog),
    rows: rows.map((row, index) => ({ key: KEYS[index] ?? "", ...row })),
    pass: choice.options.find((option) => option.kind === "decline")?.id ?? null,
    use,
  };
}

/**
 * The window on top of the action panel: its rows. A window of any question has this shape (a
 * window in a space combat too); the main button of the footer is Pass or Use. What opened it is
 * the name of the band: the heading and its context say it on the screen.
 */
export function interruptOf(window: ReactionWindow, system?: string | null): InterruptView {
  return {
    label: `Reaction · ${window.heading}`,
    source: null,
    question: null,
    link: system ? [`sys:${system}`] : [],
    blocks: [{ kind: "menu", rows: window.rows }],
  };
}

/**
 * The system that the open window is about, as the engine says it: the system or the planet of
 * its trigger, the target of the question, else the active system of the tactical action.
 */
export function windowSystem(update: SessionUpdate, choice: Choice | null): string | null {
  const trigger = update.reaction?.trigger ?? choice?.context?.trigger;
  const target = choice?.context?.target;
  const planet = trigger?.planet;
  const onMap = planet
    ? (update.view.board.map_tiles ?? []).find((tile) =>
        (tile.planets ?? []).some((item) => item.id === planet),
      )?.system_id
    : undefined;
  const aimed =
    target && "System" in target
      ? target.System
      : target && "Planet" in target
        ? target.Planet.system
        : target && "Unit" in target
          ? target.Unit.system
          : undefined;
  return trigger?.system ?? onMap ?? aimed ?? update.view.board.active_system ?? null;
}

/** The action that a window is over, as the panel shows it under the window. */
export interface ReactedAction {
  /** What the heading does not say: the kind of action and its seat, its system, or the phase. */
  context: string | null;
  heading: string;
  /** What stays in view: the card that was played, the active system. */
  blocks: BlockView[];
}

/** The events of a tactical action: the window is over the action of the seat in its system. */
const TACTICAL = new Set(["system_activated", "ship_moved", "action_completed"]);
const TACTICAL_EVENTS = new Set([
  "SYSTEM_ACTIVATED",
  "TACTICAL_ACTION_ENDED",
  "MOVEMENT_FINISHED",
  "SHIP_MOVED",
]);

/**
 * The action that opened the window, from what the engine says of its trigger: a played card
 * as printed, a strategic action with its card, a tactical action with its system. Anything else
 * is the turn of the seat, or the phase.
 */
export function reactedAction(
  update: SessionUpdate,
  trigger: DecisionTrigger | undefined,
  catalog: Catalog | null,
  choice?: Choice | null,
): ReactedAction {
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const actor = trigger?.actor ?? null;
  const mine = !!actor && actor === viewer;
  const who = actor ? (mine ? "You" : seatLabel(update, actor, catalog)) : "";
  const card = trigger?.card;
  const kind = trigger?.kind;
  if (
    card &&
    (kind === "action_card_played" ||
      kind === "action_card_discarded" ||
      trigger.event_type === "ACTION_CARD_PLAYED" ||
      trigger.event_type === "ACTION_CARD_DISCARDED")
  ) {
    // What the card will do, when the engine words it; else it is only not resolved yet.
    const result = previewOfCard(choice, card, update, catalog)?.text ?? "Not resolved yet.";
    const printed = printedSource({ ActionCard: card }, catalog) ?? {
      name: humanizeId(card),
      type: "Action card",
      text: "",
    };
    return {
      context: who ? `Action card · ${who}` : "Action card",
      heading: printed.name,
      blocks: [
        { kind: "source", ...printed },
        // A window "when" a card is played is before the card does anything.
        ...(trigger.relation === "when"
          ? [{ kind: "note" as const, tone: "quiet" as const, text: result }]
          : []),
      ],
    };
  }
  if (
    card &&
    (kind === "strategic_action_began" || trigger.event_type === "STRATEGIC_ACTION_BEGAN")
  ) {
    // The card as printed: its primary, then its secondary, each a source block.
    const parts = [false, true].flatMap((secondary) => {
      const part = printedSource({ StrategyCard: { card, secondary } }, catalog);
      return part?.text ? [{ kind: "source" as const, ...part }] : [];
    });
    const name = parts[0]?.name ?? strategyCard(card, catalog).name;
    return {
      context: who ? `Strategic action · ${who}` : "Strategic action",
      heading: name,
      blocks: parts,
    };
  }
  const system = trigger?.system ?? update.view.board.active_system ?? undefined;
  const tactical =
    !!trigger && (TACTICAL.has(trigger.kind) || TACTICAL_EVENTS.has(trigger.event_type));
  if (actor && system && (tactical || update.view.board.active_system)) {
    // The system is marked on the map; the context names it.
    return {
      context: systemLabel(tilesOf(update), system),
      heading: mine ? "Your tactical action" : `${who}’s tactical action`,
      blocks: [],
    };
  }
  // The turn of the seat, or the phase: the trigger says what happened in it.
  const phase = phaseName(update.view.phase);
  const said = trigger ? sentence(triggerText(trigger, update, catalog)) : null;
  return {
    context: said ?? (actor ? phase : null),
    heading: actor ? (mine ? "Your turn" : `${who}’s turn`) : phase,
    blocks: [],
  };
}

/**
 * The action that an ability interrupts, said in short under its band: the strategic action of
 * its card, the tactical action of the active player in its system, the turn of that player, or
 * the phase.
 */
export function interruptedAction(update: SessionUpdate, catalog: Catalog | null): ReactedAction {
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const actor = update.view.phase === "action" ? (update.view.active_player ?? null) : null;
  const mine = !!actor && actor === viewer;
  const who = actor ? (mine ? "You" : seatLabel(update, actor, catalog)) : "";
  const strategic = update.secondary;
  if (strategic) {
    const primary =
      strategic.primary === viewer ? "You" : seatLabel(update, strategic.primary, catalog);
    return {
      context: `Strategic action · ${primary}`,
      heading: strategic.name,
      blocks: [],
    };
  }
  const system = update.view.board.active_system;
  if (actor && system) {
    return {
      context: systemLabel(tilesOf(update), system),
      heading: mine ? "Your tactical action" : `${who}’s tactical action`,
      blocks: [],
    };
  }
  const phase = phaseName(update.view.phase);
  return {
    context: actor ? phase : null,
    heading: actor ? (mine ? "Your turn" : `${who}’s turn`) : phase,
    blocks: [],
  };
}
