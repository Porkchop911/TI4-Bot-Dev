import type { PendingChoiceDto } from "../protocol/types.ts";

/** The asked seat's own commodities, as the engine states them for a commodity offer. */
export interface CommodityStanding {
  have: number;
  max: number;
  /** Trade goods held, when the engine sent them (a convert option turns commodities into these). */
  tradeGoods: number | null;
}

const num = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

/**
 * `details.commodities` of a question that gains, replenishes, spends or converts the seat's own
 * commodities (exploration rewards, the Trade secondary, Hacan's agent, ...). `null` when absent,
 * or when it is the Trade primary's per-seat map rather than one seat's figures.
 */
export function describeCommodityStanding(
  choice: Pick<PendingChoiceDto, "details">,
): CommodityStanding | null {
  const raw = choice.details?.commodities;
  if (!raw || typeof raw !== "object") return null;
  const record = raw as Record<string, unknown>;
  const have = num(record.have);
  const max = num(record.max);
  if (have === null || max === null) return null;
  return { have, max, tradeGoods: num(record.trade_goods) };
}
