import React, { createContext, useContext, useRef, useState } from "react";
import type { GameLogEntry } from "../protocol/client.ts";
import type { PlayerView } from "../protocol/types.ts";
import { recordTradeGoodChanges, type TradeGoodChange, type TradeGoodLedger } from "./tradeGoodLedger.ts";

const LedgerContext = createContext<TradeGoodLedger>({});

/**
 * Remembers where each seat's trade goods came from while the game is open. The table only sends
 * the current amount, so the history starts when this page loads and is kept per seat.
 */
export const TradeGoodLedgerProvider: React.FC<{
  players: readonly PlayerView[];
  events?: readonly GameLogEntry[];
  children: React.ReactNode;
}> = ({ players, events = [], children }) => {
  const [ledger, setLedger] = useState<TradeGoodLedger>({});
  const seen = useRef<{ goods: Record<string, number>; lastId: string | null } | null>(null);
  const goods = Object.fromEntries(players.map((p) => [p.id, p.trade_goods]));
  const key = players.map((p) => `${p.id}:${p.trade_goods}`).join("|");
  const lastId = events[events.length - 1]?.id ?? null;
  const [lastKey, setLastKey] = useState(key);
  // Adjusting state while rendering (the documented alternative to an effect) so the entry is there
  // in the same render that shows the new amount.
  if (key !== lastKey) {
    setLastKey(key);
    if (seen.current) {
      setLedger(recordTradeGoodChanges(ledger, seen.current.goods, players, events, seen.current.lastId));
    }
  }
  if (!seen.current || key !== lastKey) seen.current = { goods, lastId };
  return <LedgerContext.Provider value={ledger}>{children}</LedgerContext.Provider>;
};

/** The recent trade-good changes of one seat, newest first (empty outside a provider). */
export function useTradeGoodChanges(seat: string | null | undefined): readonly TradeGoodChange[] {
  const ledger = useContext(LedgerContext);
  return (seat && ledger[seat]) || [];
}
