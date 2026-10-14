import React from "react";
import type { TradeReplenishView } from "../presentation/tradeReplenish.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import "./DecisionContext.css";

/** Trade primary: who can still be replenished, what each gains, and a way to stop. */
export const TradeReplenishPanel: React.FC<{
  view: TradeReplenishView;
  disabled: boolean;
  onChoose: (optionId: string) => void;
}> = ({ view, disabled, onChoose }) => {
  const display = usePlayerIdentity();
  return (
    <div className="trade-panel" data-testid="trade-replenish-panel">
      <p className="trade-panel__intro">
        Trade primary: you have gained 3 trade goods and replenished. Replenish another player,
        one at a time, or stop here.
      </p>
      <ul className="trade-panel__rows">
        {view.rows.map((row) => {
          const who = row.seat ? display(row.seat).label : row.name;
          return (
            <li key={row.option.id} className="trade-panel__row" data-testid="trade-replenish-row">
              <span className="trade-panel__who">{who}</span>
              <span className="trade-panel__figures" data-testid="trade-replenish-figures">
                {row.have}/{row.max} → {row.max}
                <span className="text-muted"> (+{row.gain})</span>
              </span>
              <button
                type="button"
                className="button button--primary button--sm"
                data-testid={`trade-replenish-${row.option.id}`}
                disabled={disabled}
                onClick={() => onChoose(row.option.id)}
              >
                Replenish {who}
              </button>
            </li>
          );
        })}
      </ul>
      <div className="trade-panel__actions">
        <button
          type="button"
          className="button button--secondary"
          data-testid="trade-replenish-done"
          disabled={disabled}
          onClick={() => onChoose(view.done.id)}
        >
          Done, nobody else
        </button>
      </div>
    </div>
  );
};
