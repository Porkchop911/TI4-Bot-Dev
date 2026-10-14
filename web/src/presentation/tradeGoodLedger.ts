import type { GameLogEntry } from "../protocol/client.ts";
import type { PlayerView } from "../protocol/types.ts";

/** One change of a seat's trade goods seen between two table updates. */
export interface TradeGoodChange {
  seat: string;
  /** Positive for a gain, negative for a spend. */
  delta: number;
  /** The seat's trade goods after the change. */
  balance: number;
  round?: number;
  /** The public log line the change came with (raw: run it through participantText). */
  source: string | null;
}

/** How many changes are kept per seat. */
export const LEDGER_LIMIT = 6;

export type TradeGoodLedger = Record<string, TradeGoodChange[]>;

/**
 * The log line a change most likely belongs to: the newest entry that arrived since the previous
 * update (the log is the only place the table says what just happened; trade goods themselves
 * carry no history). Falls back to the last few entries when the previous one is no longer listed.
 */
export function sourceOfChange(
  events: readonly GameLogEntry[],
  previousLastId: string | null,
): { source: string | null; round?: number } {
  const seen = previousLastId ? events.findIndex((entry) => entry.id === previousLastId) : -1;
  const fresh = seen >= 0 ? events.slice(seen + 1) : events.slice(-3);
  for (let i = fresh.length - 1; i >= 0; i--) {
    const entry = fresh[i];
    const text = entry.detail?.trim();
    if (text) return { source: text, round: entry.round };
  }
  const last = fresh[fresh.length - 1];
  return { source: last?.stage ?? null, round: last?.round };
}

/**
 * Adds the changes between `previous` (seat -> trade goods) and `players` to the ledger. Nothing is
 * recorded for a seat seen for the first time: its starting amount is not a gain.
 */
export function recordTradeGoodChanges(
  ledger: TradeGoodLedger,
  previous: Readonly<Record<string, number>>,
  players: readonly PlayerView[],
  events: readonly GameLogEntry[],
  previousLastId: string | null,
): TradeGoodLedger {
  let next = ledger;
  let cause: ReturnType<typeof sourceOfChange> | null = null;
  for (const player of players) {
    const before = previous[player.id];
    if (before === undefined || before === player.trade_goods) continue;
    cause ??= sourceOfChange(events, previousLastId);
    const change: TradeGoodChange = {
      seat: player.id,
      delta: player.trade_goods - before,
      balance: player.trade_goods,
      round: cause.round,
      source: cause.source,
    };
    next = { ...next, [player.id]: [change, ...(next[player.id] ?? [])].slice(0, LEDGER_LIMIT) };
  }
  return next;
}
