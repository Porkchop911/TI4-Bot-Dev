import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { humanizeId } from "../protocol/contentCatalog.ts";

export interface ReplenishRow {
  option: ChoiceOptionDto;
  /** The seat that would be replenished (a raw seat id; resolve it with the player identity). */
  seat: string | null;
  name: string;
  have: number;
  max: number;
  /** Commodities this seat gains: the difference up to its printed value. */
  gain: number;
}

export interface TradeReplenishView {
  rows: ReplenishRow[];
  done: ChoiceOptionDto;
}

const isDone = (option: ChoiceOptionDto) =>
  option.kind === "decline" || option.id === "done" || option.id === "decline";

/**
 * The Trade primary's "let another player replenish" question as a table of seats, each with
 * its commodities now and at its printed value. `null` for any other decision, or when the
 * engine sent no commodity figures.
 */
export function describeTradeReplenish(
  choice: Pick<PendingChoiceDto, "context" | "options" | "details">,
): TradeReplenishView | null {
  if (choice.context?.subtype !== "trade_choose_replenish") return null;
  const commodities = choice.details?.commodities;
  if (!commodities || typeof commodities !== "object") return null;
  const seats =
    choice.details?.seats && typeof choice.details.seats === "object"
      ? (choice.details.seats as Record<string, unknown>)
      : {};
  const done = choice.options.find(isDone);
  if (!done) return null;
  const rows: ReplenishRow[] = [];
  for (const option of choice.options) {
    if (option === done) continue;
    const figures = (commodities as Record<string, unknown>)[option.id] as
      | { have?: unknown; max?: unknown }
      | undefined;
    if (typeof figures?.have !== "number" || typeof figures.max !== "number") return null;
    const seat = seats[option.id];
    rows.push({
      option,
      seat: typeof seat === "string" ? seat : null,
      name: humanizeId(option.id),
      have: figures.have,
      max: figures.max,
      gain: Math.max(figures.max - figures.have, 0),
    });
  }
  return rows.length > 0 ? { rows, done } : null;
}
