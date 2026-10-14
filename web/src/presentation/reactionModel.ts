import type {
  BoardView,
  DecisionTriggerDto,
  GameEvent,
  PendingChoiceDto,
} from "../protocol/types.ts";
import { decodeDecisionTrigger } from "../protocol/decode.ts";
import { GENERATED_CONTENT_CATALOG } from "../protocol/generatedContentManifest.ts";
import {
  findActionCardByName,
  findActionCardMeta,
  findStrategyCardMeta,
  findTechnologyMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";

/**
 * Pure model of a reaction dialog: what happened, why the viewer may react, and what each
 * reaction does. The component only renders it.
 */

export type ReactionRelation = "when" | "after";

/** Where the "what happened" facts came from, best first. */
export type ReactionTriggerSource = "trigger" | "log" | "subtype";

export interface ReactionCard {
  id: string | null;
  name: string;
  /** The full printed effect text; empty when the catalog has none. */
  text: string;
  /** The printed timing window of the card, when it has one. */
  window: string | null;
}

export interface ReactionRow {
  optionId: string;
  /** The engine option kind (`ability`, `action_card`, ...). */
  kind: string;
  name: string;
  /** "Play Sabotage"; built from the card name, never from the engine label. */
  buttonLabel: string;
  card: ReactionCard | null;
  /** The printed window or ability text for a non-card option. */
  note: string | null;
  /** What playing it would do right now, from public state ("You would gain 3 commodities"). */
  effect: string | null;
}

export interface ReactionTriggerFacts {
  eventType: string | null;
  actorId: string | null;
  /** Second seat: a transaction partner, the defender in a combat. */
  subjectId: string | null;
  card: ReactionCard | null;
  /** A strategy card id, for strategy card events. */
  strategyCardId: string | null;
  agendaId: string | null;
  systemId: string | null;
  planetId: string | null;
  units: readonly { owner: string; unit_type: string; count: number }[];
  hits: number | null;
}

export interface ReactionModel {
  relation: ReactionRelation | null;
  eventType: string | null;
  source: ReactionTriggerSource;
  facts: ReactionTriggerFacts;
  /** One plain-language sentence; always present. */
  sentence: string;
  /** "Anna played an action card": the dialog title. */
  title: string;
  reactions: ReactionRow[];
  /** "Now you can play Sabotage." */
  canNowSentence: string;
  /** Extra public context for the trigger ("You have 2 units there."), when known. */
  note: string | null;
  /** This was played in response to another reaction. */
  inResponse: boolean;
  declineOptionId: string | null;
}

export interface ReactionModelInput {
  choice: PendingChoiceDto;
  /** The public log, oldest first. Used when the decision carries no trigger. */
  events?: readonly GameEvent[];
  /** Public state: whose turn it is and which system is active. */
  activePlayerId?: string | null;
  activeSystemId?: string | null;
  viewerSeat?: string | null;
  /** A player's display label ("Anna"). */
  playerLabel: (id: string) => string;
  /** "System 27 (Lodor)"; defaults to "System 27". */
  systemLabel?: (id: string) => string;
  /** The public board, to say how many of the viewer's units stand in the system involved. */
  board?: BoardView;
  /** Each seat's commodities (public), to say what Salvage would take. */
  commodities?: (seat: string) => number | null;
}

const SUBTYPE = /^(?:play_)?reaction_(when|after)_(.+)$/;

/** Reaction-window abilities that are not cards, with the window they answer. */
const ABILITY_SUBTYPES: Record<string, { relation: ReactionRelation; eventType: string }> = {
  instinct_training_cancel: { relation: "when", eventType: "ACTION_CARD_PLAYED" },
  l1z1x_agent_swap: { relation: "after", eventType: "SYSTEM_ACTIVATED" },
};

/** Inner card pick and the two non-card reaction abilities always get the reaction dialog. */
export function isReactionStepSubtype(subtype: string | undefined): boolean {
  return Boolean(subtype && (subtype.startsWith("play_reaction_") || subtype in ABILITY_SUBTYPES));
}

export function parseReactionSubtype(
  subtype: string | undefined,
): { relation: ReactionRelation; eventType: string; inner: boolean } | null {
  const ability = subtype ? ABILITY_SUBTYPES[subtype] : undefined;
  if (ability) return { ...ability, inner: true };
  const match = subtype ? SUBTYPE.exec(subtype) : null;
  if (!match) return null;
  return {
    relation: match[1] as ReactionRelation,
    eventType: match[2].toUpperCase(),
    inner: subtype!.startsWith("play_"),
  };
}

/** Event types in the words of the fallback line: "after a ship was destroyed". */
const EVENT_PHRASES: Record<string, string> = {
  ACTION_CARD_PLAYED: "an action card is played",
  ACTION_CARD_DISCARDED: "an action card is discarded",
  SYSTEM_ACTIVATED: "a system is activated",
  SHIP_MOVED: "ships move into a system",
  STRATEGIC_ACTION_BEGAN: "a strategic action begins",
  STRATEGY_CARD_CHOSEN: "a strategy card is chosen",
  STRATEGY_PHASE_BEGAN: "the strategy phase begins",
  TURN_BEGAN: "a turn begins",
  TURN_PASSED: "a turn ends",
  PLAYER_PASSED: "a player passes",
  ACTION_COMPLETED: "an action is completed",
  STRATEGY_CARDS_WOULD_RETURN: "strategy cards would be returned",
  AGENDA_PHASE_BEGAN: "the agenda phase begins",
  AGENDA_REVEALED: "an agenda is revealed",
  VOTES_CAST: "votes are cast",
  AGENDA_RESOLVED: "an agenda is resolved",
  TRANSACTION_OPENED: "a transaction is opened",
  TRANSACTION_RESOLVED: "a transaction is resolved",
  PLANET_CONTROL_GAINED: "a planet changes control",
  INVASION_BEGAN: "an invasion begins",
  UNITS_COMMITTED: "ground forces are committed",
  GROUND_ROLLS_MADE: "ground combat dice are rolled",
  SPACE_COMBAT_STARTED: "a space combat begins",
  COMBAT_ROUND_STARTED: "a combat round begins",
  ANTI_FIGHTER_BARRAGE_STARTED: "anti-fighter barrage begins",
  SPACE_CANNON_HITS: "space cannon hits are scored",
  HITS_TO_ASSIGN: "hits are about to be assigned",
  SUSTAIN_DAMAGE_USED: "a unit sustains damage",
  SHIP_DESTROYED: "a ship is destroyed",
  RETREAT_STEP_STARTED: "a retreat begins",
  RETREAT_DECLARED: "a retreat is declared",
  SPACE_COMBAT_WON: "a space combat is won",
  PRODUCTION_USED: "units are produced",
  UNIT_ABILITY_ROLLED: "a unit ability is rolled",
};

export function describeEventPhrase(eventType: string): string {
  return (
    EVENT_PHRASES[eventType] ??
    eventType
      .toLowerCase()
      .split("_")
      .join(" ")
  );
}

export function emptyFacts(eventType: string | null): ReactionTriggerFacts {
  return {
    eventType,
    actorId: null,
    subjectId: null,
    card: null,
    strategyCardId: null,
    agendaId: null,
    systemId: null,
    planetId: null,
    units: [],
    hits: null,
  };
}

export function cardFromId(id: string | null | undefined, name?: string | null): ReactionCard | null {
  const meta = id ? findActionCardMeta(id) : name ? findActionCardByName(name) : undefined;
  const resolvedName = meta?.name ?? name ?? (id ? humanizeId(id) : null);
  if (!resolvedName) return null;
  return {
    id: meta?.id ?? id ?? null,
    name: resolvedName,
    text: meta?.description ?? "",
    window: meta?.window ?? null,
  };
}

/** Facts from the public log and public state, for decisions that carry no trigger. */
export function factsFromLog(
  eventType: string | null,
  input: Pick<ReactionModelInput, "events" | "activePlayerId" | "activeSystemId" | "choice">,
): ReactionTriggerFacts | null {
  const facts = emptyFacts(eventType);
  let found = false;
  if (eventType === "ACTION_CARD_PLAYED") {
    const entries = input.events ?? [];
    for (let index = entries.length - 1; index >= 0; index -= 1) {
      const entry = entries[index];
      const match = entry.detail ? / played (.+?)\.?$/.exec(entry.detail) : null;
      if (!match || !entry.actor || entry.actor === input.choice.actor) continue;
      facts.actorId = entry.actor;
      facts.card = cardFromId(null, match[1]);
      return facts;
    }
    return null;
  }
  const activeKinds = new Set([
    "SYSTEM_ACTIVATED",
    "SHIP_MOVED",
    "INVASION_BEGAN",
    "UNITS_COMMITTED",
    "PRODUCTION_USED",
    "SPACE_COMBAT_STARTED",
    "COMBAT_ROUND_STARTED",
  ]);
  if (eventType && activeKinds.has(eventType)) {
    if (input.activePlayerId) {
      facts.actorId = input.activePlayerId;
      found = true;
    }
    if (input.activeSystemId) {
      facts.systemId = input.activeSystemId;
      found = true;
    }
  }
  if (
    eventType &&
    ["TURN_BEGAN", "TURN_PASSED", "ACTION_COMPLETED", "STRATEGIC_ACTION_BEGAN"].includes(eventType) &&
    input.activePlayerId
  ) {
    facts.actorId = input.activePlayerId;
    found = true;
  }
  return found ? facts : null;
}

const UNIT_PLURALS: Record<string, string> = {
  infantry: "infantry",
  fighter: "fighters",
  destroyer: "destroyers",
  cruiser: "cruisers",
  carrier: "carriers",
  dreadnought: "dreadnoughts",
  warsun: "war suns",
  war_sun: "war suns",
  flagship: "flagships",
  mech: "mechs",
  pds: "PDS",
  spacedock: "space docks",
  space_dock: "space docks",
};

function unitPhrase(units: ReactionTriggerFacts["units"]): string {
  return units
    .map(({ unit_type, count }) => {
      const base = unit_type.toLowerCase();
      const name = count === 1 ? humanizeId(base).toLowerCase() : (UNIT_PLURALS[base] ?? `${base}s`);
      return `${count} ${name}`;
    })
    .join(", ");
}

function strategyCardName(id: string | null): string {
  return id ? (findStrategyCardMeta(id)?.name ?? humanizeId(id)) : "a";
}

/** Facts from the engine's typed trigger: the best source, and the only one for exact units. */
export function factsFromTrigger(trigger: DecisionTriggerDto): ReactionTriggerFacts {
  const facts = emptyFacts(trigger.event_type);
  facts.actorId = trigger.actor ?? null;
  facts.subjectId = trigger.subject ?? null;
  if (trigger.card) {
    if (trigger.kind === "strategic_action_began" || trigger.kind === "strategy_card_chosen") {
      facts.strategyCardId = trigger.card;
    } else {
      facts.card = cardFromId(trigger.card);
    }
  }
  facts.agendaId = trigger.agenda ?? null;
  facts.systemId = trigger.system ?? null;
  facts.planetId = trigger.planet ?? null;
  facts.units = trigger.units ?? [];
  facts.hits = trigger.hits ?? null;
  return facts;
}

/** "You have 2 units there." for the system the trigger names, from the public board. */
export function viewerPresenceNote(
  board: BoardView | undefined,
  systemId: string | null,
  viewerSeat: string | null | undefined,
): string | null {
  if (!board || !systemId || !viewerSeat) return null;
  const system = board.systems?.[systemId];
  if (!system) return null;
  const count = system.units.filter((unit) => unit.owner === viewerSeat).length;
  if (count === 0) return null;
  return `You have ${count} unit${count === 1 ? "" : "s"} there.`;
}

/** The "what happened" sentence for the facts, with `A` the actor's label. */
export function triggerSentence(
  facts: ReactionTriggerFacts,
  relation: ReactionRelation | null,
  labels: { actor: (id: string) => string; system: (id: string) => string; viewerSeat?: string | null },
): string {
  const eventType = facts.eventType ?? "";
  const you = facts.actorId !== null && facts.actorId === labels.viewerSeat;
  const actor = facts.actorId ? labels.actor(facts.actorId) : null;
  const A = actor ?? "A player";
  const be = you ? "are" : "is";
  const poss = you ? "your" : `${A}'s`;
  const system = facts.systemId ? labels.system(facts.systemId) : null;
  const subject = facts.subjectId ? labels.actor(facts.subjectId) : null;
  const inSystem = system ? ` in ${system}` : "";
  switch (eventType) {
    case "ACTION_CARD_PLAYED":
      return facts.card
        ? `${A} played the action card ${facts.card.name}.`
        : `${A} played an action card.`;
    case "ACTION_CARD_DISCARDED":
      return facts.card
        ? `${A} discarded the action card ${facts.card.name}.`
        : `${A} discarded an action card.`;
    case "SYSTEM_ACTIVATED":
      return `${A} activated ${system ?? "a system"}.`;
    case "SHIP_MOVED": {
      const units = unitPhrase(facts.units);
      return `${A} moved ships into ${system ?? "a system"}${units ? `: ${units}` : ""}.`;
    }
    case "STRATEGIC_ACTION_BEGAN":
      return facts.strategyCardId
        ? `${A} ${be} about to use the ${strategyCardName(facts.strategyCardId)} strategy card.`
        : `${A} ${be} about to use a strategy card.`;
    case "STRATEGY_CARD_CHOSEN":
      return facts.strategyCardId
        ? `${A} chose the ${strategyCardName(facts.strategyCardId)} strategy card.`
        : `${A} chose a strategy card.`;
    case "STRATEGY_PHASE_BEGAN":
      return "The strategy phase began.";
    case "TURN_BEGAN":
      return actor ? `It is ${poss} turn.` : "A turn began.";
    case "TURN_PASSED":
      return `${A} ended ${you ? "your" : "their"} turn.`;
    case "PLAYER_PASSED":
      return `${A} passed.`;
    case "ACTION_COMPLETED":
      return `${A} finished an action.`;
    case "STRATEGY_CARDS_WOULD_RETURN":
      return "Strategy cards are about to be returned.";
    case "AGENDA_PHASE_BEGAN":
      return "The agenda phase began.";
    case "AGENDA_REVEALED":
      return facts.agendaId
        ? `The agenda ${humanizeId(facts.agendaId)} was revealed.`
        : "An agenda was revealed.";
    case "VOTES_CAST":
      return `${A} cast ${you ? "your" : "their"} votes.`;
    case "AGENDA_RESOLVED":
      return "An agenda outcome is about to be resolved.";
    case "TRANSACTION_OPENED":
    case "TRANSACTION_RESOLVED":
      return subject
        ? `${A} ${eventType === "TRANSACTION_OPENED" ? "opened" : "completed"} a transaction with ${subject}.`
        : `${A} ${eventType === "TRANSACTION_OPENED" ? "opened" : "completed"} a transaction.`;
    case "PLANET_CONTROL_GAINED":
      return `${A} gained control of ${facts.planetId ? humanizeId(facts.planetId) : "a planet"}${inSystem}.`;
    case "INVASION_BEGAN":
      return `${A} began an invasion${system ? ` of ${system}` : ""}.`;
    case "UNITS_COMMITTED":
      return `${A} committed ground forces${inSystem}.`;
    case "GROUND_ROLLS_MADE":
      return `Ground combat dice were rolled${inSystem}.`;
    case "SPACE_COMBAT_STARTED":
      return `A space combat began${inSystem}.`;
    case "COMBAT_ROUND_STARTED":
      return `A combat round began${inSystem}.`;
    case "ANTI_FIGHTER_BARRAGE_STARTED":
      return `Anti-fighter barrage began${inSystem}.`;
    case "SPACE_CANNON_HITS":
      return `Space cannon fire scored ${facts.hits ?? "some"} hits${inSystem}.`;
    case "HITS_TO_ASSIGN":
      return `${facts.hits ?? "Some"} hits are about to be assigned${inSystem}.`;
    case "SUSTAIN_DAMAGE_USED":
      return `A unit sustained damage${inSystem}.`;
    case "SHIP_DESTROYED":
      return `A ship was destroyed${inSystem}.`;
    case "RETREAT_STEP_STARTED":
    case "RETREAT_DECLARED":
      return `${A} ${be} retreating${inSystem}.`;
    case "SPACE_COMBAT_WON":
      return `${A} won a space combat${inSystem}.`;
    case "PRODUCTION_USED":
      return `${A} produced units${inSystem}.`;
    case "UNIT_ABILITY_ROLLED":
      return `A unit ability was rolled${inSystem}.`;
    default: {
      const phrase = describeEventPhrase(eventType || "event");
      return `A reaction window opened: ${relation ?? "after"} ${phrase}.`;
    }
  }
}

/** A title for the dialog derived from the sentence's subject, never a raw id. */
export function triggerTitle(
  facts: ReactionTriggerFacts,
  relation: ReactionRelation | null,
  labels: { actor: (id: string) => string; viewerSeat?: string | null },
): string {
  const A = facts.actorId
    ? facts.actorId === labels.viewerSeat
      ? "You"
      : labels.actor(facts.actorId)
    : null;
  switch (facts.eventType) {
    case "ACTION_CARD_PLAYED":
      return A ? `${A} played an action card` : "An action card was played";
    case "SYSTEM_ACTIVATED":
      return A ? `${A} activated a system` : "A system was activated";
    case "SHIP_MOVED":
      return A ? `${A} moved ships` : "Ships moved";
    case "STRATEGIC_ACTION_BEGAN":
      return A ? `${A} begins a strategic action` : "A strategic action begins";
    case "AGENDA_REVEALED":
      return "Agenda revealed";
    default:
      return relation === "when" ? "Reaction before it resolves" : "Reaction window";
  }
}

function stripPlay(label: string): string {
  return label.replace(/^play\s+/i, "").trim();
}

interface CatalogNamed {
  name: string;
  text?: string;
  abilityText?: string;
  ability?: string;
}

const CATALOG_NOTES = GENERATED_CONTENT_CATALOG.promissoryNotes as unknown as Record<string, CatalogNamed>;
const CATALOG_UNITS = GENERATED_CONTENT_CATALOG.units as unknown as Record<string, CatalogNamed>;
const CATALOG_LEADERS = GENERATED_CONTENT_CATALOG.leaders as unknown as Record<string, CatalogNamed>;

/**
 * Non-card abilities (`technology:<faction>:<id>:<EVENT>:<rel>`, `leader:...`, and the faction's
 * `promissory:<owner>:<alias>:...` note and `unit:<owner>:<unit>:...` card): the printed name and
 * text from the content catalog, never the raw engine id.
 */
function abilityName(optionId: string): { name: string; note: string | null; text: string } | null {
  const match = /^(technology|leader|promissory|unit):[^:]*:([^:]+):/.exec(optionId);
  if (!match) return null;
  const [, family, id] = match;
  if (family === "technology") {
    const tech = findTechnologyMeta(id);
    return { name: tech?.name ?? humanizeId(id), note: null, text: tech?.description ?? "" };
  }
  if (family === "promissory") {
    const note = CATALOG_NOTES[id] ?? CATALOG_NOTES[`<color>_${id}`];
    return {
      name: note?.name ?? humanizeId(id),
      note: null,
      text: (note?.text ?? "").replace(/<color>/g, "issuing"),
    };
  }
  if (family === "unit") {
    const unit = CATALOG_UNITS[id];
    return { name: unit?.name ?? humanizeId(id), note: null, text: unit?.ability ?? "" };
  }
  const leader = CATALOG_LEADERS[id];
  if (leader) return { name: leader.name, note: null, text: leader.abilityText ?? "" };
  const name = humanizeId(id.replace(/(agent|commander|hero)$/, " $1").trim());
  return { name, note: null, text: "" };
}

/** The use/pass step of Instinct Training and the L1Z1X agent. */
function abilityStepRow(
  option: PendingChoiceDto["options"][number],
  subtype: string,
): ReactionRow {
  const kind = option.kind ?? "";
  if (subtype === "instinct_training_cancel") {
    const tech = findTechnologyMeta("it");
    return {
      optionId: option.id,
      kind,
      name: tech?.name ?? "Instinct Training",
      buttonLabel: `Use ${tech?.name ?? "Instinct Training"}`,
      card: null,
      effect: null,
      note: tech?.description || "Exhaust this card and spend 1 token from your strategy pool: cancel that action card.",
    };
  }
  return {
    optionId: option.id,
    kind,
    name: "L1Z1X agent",
    buttonLabel: "Use L1Z1X agent",
    card: null,
    effect: null,
    note: "Exhaust this card to let that player replace 1 of their infantry in the active system with 1 mech from their reinforcements.",
  };
}

function rowFor(option: PendingChoiceDto["options"][number]): ReactionRow {
  const kind = option.kind ?? "";
  const payload = option.payload ?? {};
  const cardId = typeof payload.card === "string" ? payload.card : null;
  const cardName = typeof payload.card_name === "string" ? payload.card_name : null;
  if (cardId || cardName) {
    const card = cardFromId(cardId, cardName);
    const name = cardName ?? card?.name ?? stripPlay(option.label);
    return {
      optionId: option.id,
      kind,
      name,
      buttonLabel: `Play ${name}`,
      card,
      effect: null,
      note: null,
    };
  }
  const ability = abilityName(option.id);
  if (ability) {
    return {
      optionId: option.id,
      kind,
      name: ability.name,
      buttonLabel: `Use ${ability.name}`,
      card: null,
      effect: null,
      note: ability.text || null,
    };
  }
  // The outer offer for several differently named cards ("Choose an action card…") or an
  // unknown option: show the engine label once, without a doubled verb.
  const label = stripPlay(option.label);
  const meta = findActionCardByName(label);
  if (meta) {
    const card = cardFromId(meta.id, meta.name);
    return { optionId: option.id, kind, name: meta.name, buttonLabel: `Play ${meta.name}`, card, effect: null, note: null };
  }
  const choosing = /^choose\b/i.test(label);
  return {
    optionId: option.id,
    kind,
    name: label,
    buttonLabel: choosing ? label : `Play ${label}`,
    card: null,
    effect: null,
    note: null,
  };
}

export function isDeclineOption(option: { id: string; kind?: string }): boolean {
  return option.id === "decline" || option.kind === "decline";
}

/**
 * Salvage ("Your opponent gives you all of their commodities"): the opponents are the sides of the
 * combat the board still names (the losers' ships are gone), and their commodities are public.
 */
export function salvageEffect(
  winner: string | null,
  board: BoardView | undefined,
  commodities: ReactionModelInput["commodities"],
  playerLabel: (id: string) => string,
): string | null {
  const combat = board?.combat;
  if (!winner || !combat || !commodities) return null;
  const opponents = [combat.attacker, combat.defender].filter((seat) => seat && seat !== winner);
  if (opponents.length === 0) return null;
  const parts: { seat: string; count: number }[] = [];
  for (const seat of opponents) {
    const count = commodities(seat);
    if (count === null) return null;
    parts.push({ seat, count });
  }
  const total = parts.reduce((sum, part) => sum + part.count, 0);
  if (total === 0) {
    return `${parts.map((part) => playerLabel(part.seat)).join(" and ")} hold no commodities, so you would gain nothing.`;
  }
  const noun = `${total} commodit${total === 1 ? "y" : "ies"}`;
  const detail =
    parts.length > 1
      ? ` (${parts.map((part) => `${playerLabel(part.seat)} ${part.count}`).join(", ")})`
      : ` from ${playerLabel(parts[0].seat)}`;
  return `You would gain ${noun}${detail}, as trade goods.`;
}

function joinNames(names: string[]): string {
  if (names.length <= 1) return names[0] ?? "";
  return `${names.slice(0, -1).join(", ")} or ${names[names.length - 1]}`;
}

export function describeReaction(input: ReactionModelInput): ReactionModel {
  const { choice } = input;
  const parsed = parseReactionSubtype(choice.context?.subtype);
  const trigger = decodeDecisionTrigger(choice.context?.trigger);
  const relation = trigger?.relation ?? parsed?.relation ?? null;
  const eventType = trigger?.event_type ?? parsed?.eventType ?? null;
  const systemLabel = input.systemLabel ?? ((id: string) => `System ${id}`);
  const labels = {
    actor: (id: string) => (id === input.viewerSeat ? "You" : input.playerLabel(id)),
    system: systemLabel,
    viewerSeat: input.viewerSeat,
  };

  let source: ReactionTriggerSource = "subtype";
  let facts = emptyFacts(eventType);
  let relationUsed = relation;
  const fromLog = trigger ? null : factsFromLog(eventType, input);
  if (trigger) {
    facts = factsFromTrigger(trigger);
    source = "trigger";
    relationUsed = trigger.relation;
  } else if (fromLog) {
    facts = fromLog;
    source = "log";
  }

  const subtype = choice.context?.subtype ?? "";
  const reactions = choice.options
    .filter((option) => !isDeclineOption(option))
    .map((option) =>
      subtype in ABILITY_SUBTYPES ? abilityStepRow(option, subtype) : rowFor(option),
    );
  for (const row of reactions) {
    if (row.card?.id === "salvage") {
      row.effect = salvageEffect(
        trigger?.actor ?? input.viewerSeat ?? null,
        input.board,
        input.commodities,
        input.playerLabel,
      );
    }
  }
  const declineOptionId = choice.options.find(isDeclineOption)?.id ?? null;
  // Level 3: nothing is known beyond the window, so say only that.
  const sentence =
    source === "subtype"
      ? `A reaction window opened: ${relation ?? "after"} ${describeEventPhrase(eventType ?? "event")}.`
      : triggerSentence(facts, relationUsed, labels);
  const title =
    source === "subtype" ? "Reaction window" : triggerTitle(facts, relationUsed, labels);
  const names = reactions.map((row) => row.name);
  const lead = relationUsed === "when" ? "Before this resolves, you can" : "Now you can";
  const canNowSentence = names.length
    ? `${lead} ${joinNames(reactions.map((row) => (/^choose\b/i.test(row.name) ? row.name.toLowerCase() : row.buttonLabel.replace(/^Play /, "play ").replace(/^Use /, "use "))))}.`
    : `${lead} react.`;

  return {
    relation,
    eventType,
    source,
    facts,
    sentence,
    title,
    reactions,
    canNowSentence,
    note:
      source === "subtype" || facts.eventType !== "SYSTEM_ACTIVATED"
        ? null
        : viewerPresenceNote(input.board, facts.systemId, input.viewerSeat),
    inResponse: (trigger?.chain?.length ?? 0) > 0,
    declineOptionId,
  };
}
