import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { readReadyHints, type ReadyHints } from "./readyHints.ts";
import { findStrategyCardMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { describeCommodityStanding, type CommodityStanding } from "./commodityStanding.ts";

export interface StrategySecondaryView {
  cardName: string;
  initiative: number | null;
  /** The seat that played the card (a raw seat id; resolve it with the player identity). */
  playedBy: string | null;
  /** What the secondary does, in the card's printed words. */
  secondaryText: string | null;
  tokensLeft: number | null;
  costsToken: boolean;
  yes: ChoiceOptionDto;
  no: ChoiceOptionDto;
  /** A button caption that says what happens, e.g. "Spend 1 strategy token to draw two action cards". */
  yesLabel: string;
  noLabel: string;
  /** Diplomacy: the planets that could be readied and which spending cards were played. */
  readyHints: ReadyHints | null;
  /** Trade: the follower's commodities now and the limit a replenish fills to. */
  commodities: CommodityStanding | null;
}

const isNo = (option: ChoiceOptionDto) =>
  option.id === "no" || option.id === "decline" || option.kind === "decline";

/** "spend a strategy token and 4 resources to research" -> "Spend 1 strategy token + 4 resources to research". */
export function secondaryButtonLabel(prompt: string, fallback: string): string {
  const strategy = /^spend a strategy token(?: and (.+?))? to (.+)$/i.exec(prompt.trim());
  if (strategy) {
    const extra = strategy[1] ? ` + ${strategy[1]}` : "";
    return `Spend 1 strategy token${extra} to ${strategy[2]}`;
  }
  const influence = /^spend (\d+) influence for a command token$/i.exec(prompt.trim());
  if (influence) return `Spend ${influence[1]} influence for a command token`;
  return fallback;
}

/**
 * A strategy card secondary offer as a named action: which card, who played it, what it does and
 * what it costs. `null` for any other decision, or when the options are not a yes/no pair.
 */
export function describeStrategySecondary(
  choice: Pick<PendingChoiceDto, "prompt" | "options" | "details">,
): StrategySecondaryView | null {
  const details = choice.details;
  if (!details || details.kind !== "strategy_secondary" || typeof details.card !== "string") {
    return null;
  }
  const no = choice.options.find(isNo);
  const yes = choice.options.find((option) => option !== no);
  if (!no || !yes || choice.options.length !== 2) return null;
  const meta = findStrategyCardMeta(details.card);
  const tokensLeft = typeof details.tokens_left === "number" ? details.tokens_left : null;
  return {
    cardName: meta?.name ?? humanizeId(details.card),
    initiative: meta?.initiative ?? null,
    playedBy: typeof details.played_by === "string" ? details.played_by : null,
    secondaryText: meta?.secondaryText || null,
    tokensLeft,
    costsToken: details.costs_token !== false,
    yes,
    no,
    yesLabel: secondaryButtonLabel(choice.prompt, yes.label),
    noLabel: "Skip",
    readyHints: readReadyHints(details),
    commodities: describeCommodityStanding(choice),
  };
}
