import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";

/** Hacan's commander: spend trade goods for extra votes on the outcome you just voted for. */
export interface VoteGoodsView {
  /** The commander card as printed (name, window, text). */
  title: string;
  window: string | null;
  text: string | null;
  /** The outcome being voted for, by name ("For", "Jord"). */
  outcome: string;
  votes: number;
  goods: number;
  votesPerGood: number;
  /** The option that spends `n` trade goods, by n. */
  spend: ReadonlyMap<number, ChoiceOptionDto>;
  decline: ChoiceOptionDto;
}

const text = (value: unknown): string | null =>
  typeof value === "string" && value.trim() ? value.trim() : null;

const count = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : null;

/** "spend|3" -> 3 */
const spentGoods = (option: ChoiceOptionDto): number | null => {
  const match = /^spend\|(\d+)$/.exec(option.id);
  return match ? Number(match[1]) : null;
};

/**
 * The "spend trade goods for votes" question as a named amount picker. `null` for any other
 * decision, or without the engine's details.
 */
export function describeVoteGoods(
  choice: Pick<PendingChoiceDto, "options" | "details">,
): VoteGoodsView | null {
  const details = choice.details;
  if (!details || details.kind !== "vote_trade_goods") return null;
  const card = (details.card ?? {}) as Record<string, unknown>;
  const outcomeId = text(details.outcome);
  const votes = count(details.votes);
  const goods = count(details.goods);
  const votesPerGood = count(details.votes_per_good);
  const decline = choice.options.find((o) => o.kind === "decline" || o.id === "decline");
  if (!outcomeId || votes === null || goods === null || votesPerGood === null || !decline) {
    return null;
  }
  const spend = new Map<number, ChoiceOptionDto>();
  for (const option of choice.options) {
    const n = spentGoods(option);
    if (n !== null) spend.set(n, option);
  }
  if (spend.size === 0) return null;
  return {
    title: text(card.title) ?? "Spend trade goods for votes",
    window: text(card.window),
    text: text(card.text),
    outcome: findPlanetMeta(outcomeId)?.name ?? humanizeId(outcomeId),
    votes,
    goods,
    votesPerGood,
    spend,
    decline,
  };
}

/** "Spend 2 trade goods for 4 more votes (7 → 11)". */
export function voteGoodsLabel(view: VoteGoodsView, n: number): string {
  const gained = n * view.votesPerGood;
  return `Spend ${n} trade good${n === 1 ? "" : "s"} for ${gained} more votes (${view.votes} → ${view.votes + gained})`;
}
