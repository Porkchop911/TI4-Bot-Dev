import type { PendingChoiceDto } from "../protocol/types.ts";
import {
  findActionCardMeta,
  findBreakthroughMeta,
  findStrategyCardMeta,
  findTechnologyMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";

function contentName(alias: string): string {
  return (
    findTechnologyMeta(alias)?.name ??
    findActionCardMeta(alias)?.name ??
    findStrategyCardMeta(alias)?.name ??
    findBreakthroughMeta(alias)?.name ??
    humanizeId(alias)
  );
}

/** A readable name for the decision's structured `context.source`. */
export function formatDecisionSource(source: Record<string, unknown> | undefined): string | null {
  if (!source || typeof source !== "object") return null;
  const [key, value] = Object.entries(source)[0] ?? [];
  if (!key) return null;
  if (key === "StrategyCard" && value && typeof value === "object") {
    const card = (value as { card?: unknown; secondary?: unknown }).card;
    const secondary = Boolean((value as { secondary?: unknown }).secondary);
    if (typeof card !== "string") return null;
    const name = findStrategyCardMeta(card)?.name ?? humanizeId(card);
    return secondary ? `${name} (secondary)` : name;
  }
  if (typeof value !== "string" || !value) return null;
  switch (key) {
    case "ActionCard":
      return findActionCardMeta(value)?.name ?? humanizeId(value);
    case "Rule":
      return `Rule ${value}`;
    case "FactionAbility":
    case "Agenda":
    case "Reaction":
      return findBreakthroughMeta(value)?.name ?? humanizeId(value);
    default:
      return contentName(value);
  }
}

/** What a decision is about, in the words a player uses, keyed by the engine's stable subtype. */
const SUBTYPE_TOPICS: Record<string, string> = {
  gain_command_token: "Command tokens",
  buy_token_with_influence: "Buy a command token",
  status_redistribute_tokens: "Redistribute command tokens",
  warfare_redistribute_tokens: "Redistribute command tokens",
  predictive_intelligence_redistribute: "Redistribute command tokens",
  discard_over_hand_limit: "Action card hand limit",
  return_over_secret_hand_limit: "Secret objective limit",
  politics_place_agenda: "Politics",
  politics_choose_speaker: "Politics",
  diplomacy_choose_system: "Diplomacy",
  trade_choose_replenish: "Trade",
  warfare_recall_token: "Warfare",
  research_technology: "Research",
  draft_strategy_card: "Strategy phase",
  strategy_secondary: "Strategy card secondary",
  ready_planet: "Ready a planet",
  place_structure: "Construction",
  construction_choose_ability: "Construction",
  imperial_score_objective: "Imperial",
  cast_vote: "Agenda vote",
  vote_exhaust_planet: "Agenda vote",
  vote_tiebreak: "Agenda vote",
  propose_transaction: "Transaction",
  answer_transaction: "Transaction",
  remove_custodians: "Mecatol Rex",
  commit_ground_forces: "Invasion",
  produce_unit: "Production",
  pay_resources: "Payment",
  pay_influence: "Payment",
};

export interface DecisionHeaderInfo {
  /** The line above the question: the card or ability asking, and the topic. */
  eyebrow: string | null;
  /** Short facts about when and why this is asked. */
  chips: string[];
}

function phaseLabel(phase: string | undefined): string | null {
  if (!phase) return null;
  const word = humanizeId(phase.toLowerCase());
  return `${word} phase`;
}

/**
 * The context strip of a decision, built only from what the server already sends: the structured
 * source, the stable subtype, the phase and the round.
 */
export function describeDecisionHeader(
  choice: Pick<PendingChoiceDto, "context">,
): DecisionHeaderInfo {
  const context = choice.context;
  if (!context) return { eyebrow: null, chips: [] };
  const sourceKey = context.source ? Object.keys(context.source)[0] : undefined;
  const formatted = formatDecisionSource(context.source);
  const rule = sourceKey === "Rule" ? formatted : null;
  const sourceName = sourceKey === "Rule" ? null : formatted;
  const topic = SUBTYPE_TOPICS[context.subtype] ?? null;
  // "Politics (secondary)" already says "Politics": do not repeat the topic after it.
  const topicRepeated =
    sourceName !== null && topic !== null && sourceName.toLowerCase().startsWith(topic.toLowerCase());
  const eyebrow = [sourceName, topicRepeated ? null : topic].filter(
    (part, index, all): part is string => Boolean(part) && all.indexOf(part) === index,
  );
  const chips: string[] = [];
  const phase = phaseLabel(context.phase);
  if (phase) chips.push(context.round ? `${phase} · Round ${context.round}` : phase);
  if (rule) chips.push(rule);
  if (context.optional) chips.push("Optional");
  return { eyebrow: eyebrow.length ? eyebrow.join(" · ") : null, chips };
}

/**
 * The breakthrough a decision was raised by, if any: its structured `context.source` names the
 * card's alias (`Content`, `FactionAbility` or `Reaction`), and the card's printed text belongs on
 * the decision so the player is not asked about a card they cannot read.
 */
export function decisionBreakthrough(
  choice: Pick<PendingChoiceDto, "context">,
): ReturnType<typeof findBreakthroughMeta> {
  const source = choice.context?.source;
  if (!source || typeof source !== "object") return undefined;
  const value = Object.values(source)[0];
  return typeof value === "string" ? findBreakthroughMeta(value) : undefined;
}
