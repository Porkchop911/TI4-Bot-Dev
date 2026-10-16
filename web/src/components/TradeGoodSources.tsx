import React from "react";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { useTradeGoodChanges } from "../presentation/TradeGoodLedgerContext.tsx";

/**
 * Where a seat's trade goods came from: the last few changes seen since this page opened, each
 * with the log line it arrived with. The table keeps no history of its own, so before the first
 * change the list says so instead of guessing.
 */
export const TradeGoodSources: React.FC<{ seat: string; limit?: number }> = ({ seat, limit = 4 }) => {
  const changes = useTradeGoodChanges(seat).slice(0, limit);
  const text = useParticipantText();
  if (!changes.length) {
    return (
      <p className="trade-good-sources text-muted" data-testid="trade-good-sources-empty">
        No trade goods gained or spent since you opened this game.
      </p>
    );
  }
  return (
    <ul className="trade-good-sources" data-testid="trade-good-sources" aria-label="Recent trade goods changes">
      {changes.map((change, index) => (
        <li key={index} className="trade-good-sources__row" data-gain={change.delta > 0 ? "true" : "false"}>
          <strong className="trade-good-sources__delta">
            {change.delta > 0 ? `+${change.delta}` : `−${Math.abs(change.delta)}`}
          </strong>
          <span className="trade-good-sources__text">
            {change.source ? text(change.source) : "reason not logged"}
            {change.round ? ` (round ${change.round})` : ""}
          </span>
          <span className="trade-good-sources__balance text-muted">now {change.balance}</span>
        </li>
      ))}
    </ul>
  );
};
