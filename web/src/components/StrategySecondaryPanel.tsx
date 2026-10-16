import React from "react";
import { ValueText } from "./PlanetValueIcons.tsx";
import type { StrategySecondaryView } from "../presentation/strategySecondary.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { usePreparedHint } from "../presentation/PreparedHint.tsx";
import { ReadyHintsPanel } from "./ReadyHintsPanel.tsx";
import { CommodityStandingFacts } from "./CommodityStandingFacts.tsx";
import "./DecisionContext.css";

/** A strategy card's secondary as a named action: the card, who played it, what it costs. */
export const StrategySecondaryPanel: React.FC<{
  view: StrategySecondaryView;
  disabled: boolean;
  onChoose: (optionId: string) => void;
}> = ({ view, disabled, onChoose }) => {
  const display = usePlayerIdentity();
  const player = view.playedBy ? display(view.playedBy) : null;
  const hint = usePreparedHint();
  const preparedYes = hint?.optionId === view.yes.id;
  const preparedNo = hint?.optionId === view.no.id;
  const cantPay = view.costsToken && view.tokensLeft !== null && view.tokensLeft < 1;
  return (
    <div className="secondary-panel" data-testid="strategy-secondary-panel">
      <div className="secondary-panel__card">
        <div className="secondary-panel__title">
          {view.initiative ? `${view.initiative}. ` : ""}
          {view.cardName}
          <span className="secondary-panel__tag">secondary</span>
        </div>
        {player && (
          <div className="secondary-panel__meta" data-testid="secondary-played-by">
            Played by {player.label}
          </div>
        )}
        {view.secondaryText && (
          <p className="secondary-panel__text" data-testid="secondary-text">
            {view.secondaryText}
          </p>
        )}
      </div>
      {view.readyHints && <ReadyHintsPanel hints={view.readyHints} />}
      {view.tokensLeft !== null && (
        <div className="secondary-panel__tokens" data-testid="secondary-tokens">
          <span>Strategy tokens</span>
          <span className="secondary-panel__pips" aria-hidden="true">
            {Array.from({ length: Math.max(view.tokensLeft, 0) }, (_, index) => (
              <i key={index} className="secondary-panel__pip" />
            ))}
          </span>
          <strong>
            {view.tokensLeft}
            {view.costsToken && view.tokensLeft > 0 ? ` (${view.tokensLeft - 1} after)` : ""}
          </strong>
        </div>
      )}
      {view.commodities && <CommodityStandingFacts standing={view.commodities} />}
      <div className="secondary-panel__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="secondary-yes-btn"
          data-prepared={preparedYes || undefined}
          disabled={disabled || cantPay}
          onClick={() => onChoose(view.yes.id)}
        >
          <ValueText text={view.yesLabel} />
          {preparedYes && <span className="secondary-panel__prepared"> (prepared)</span>}
        </button>
        <button
          type="button"
          className="button button--secondary"
          data-testid="secondary-skip-btn"
          data-prepared={preparedNo || undefined}
          disabled={disabled}
          onClick={() => onChoose(view.no.id)}
        >
          {view.noLabel}
          {preparedNo && <span className="secondary-panel__prepared"> (prepared)</span>}
        </button>
      </div>
    </div>
  );
};
