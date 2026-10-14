import type { GameView, PendingChoiceDto } from "../protocol/types.ts";

const subtypeOf = (choice: Pick<PendingChoiceDto, "context">) => choice.context?.subtype ?? "";

/** The subtypes whose options are strategy cards, shown as the card grid. */
export function isStrategyCardGrid(choice: Pick<PendingChoiceDto, "context">): boolean {
  const subtype = subtypeOf(choice);
  return subtype === "draft_strategy_card" || subtype.endsWith("investments_pick_strategy_card");
}

/** Manipulate Investments: which of the five trade goods this question places. */
export interface InvestmentsProgress {
  step: number;
  of: number;
  /** Different cards that still have to receive a good (at least three in all). */
  distinctOwed: number;
}

const asCount = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) && value >= 0 ? Math.floor(value) : null;

/** `null` for any other decision, or when the engine sent no progress. */
export function investmentsProgress(
  choice: Pick<PendingChoiceDto, "context" | "details">,
): InvestmentsProgress | null {
  if (!subtypeOf(choice).endsWith("investments_pick_strategy_card")) return null;
  const step = asCount(choice.details?.step);
  const of = asCount(choice.details?.of);
  const distinctOwed = asCount(choice.details?.distinct_owed);
  if (!step || !of || distinctOwed === null) return null;
  return { step, of, distinctOwed };
}

/** Trade goods lying on a strategy card now, from the table view; `null` when unknown. */
export function goodsOnCard(table: Partial<Pick<GameView, "table">> | null, cardId: string): number | null {
  const goods = table?.table?.strategy_card_goods;
  if (!goods) return null;
  return goods[cardId] ?? 0;
}

/** "2 trade goods on it now; taking it pays them out" style line for a card option. */
export function goodsNote(
  choice: Pick<PendingChoiceDto, "context" | "details">,
  table: Partial<Pick<GameView, "table">> | null,
  cardId: string,
): string | null {
  const now = goodsOnCard(table, cardId);
  if (now === null) return null;
  const unit = (n: number) => `${n} trade good${n === 1 ? "" : "s"}`;
  if (investmentsProgress(choice)) {
    return now === 0
      ? "No trade goods on it yet; placing one makes it 1."
      : `${unit(now)} on it now; placing one makes it ${now + 1}.`;
  }
  return now === 0 ? "No trade goods on it." : `${unit(now)} on it: taking this card collects them.`;
}
