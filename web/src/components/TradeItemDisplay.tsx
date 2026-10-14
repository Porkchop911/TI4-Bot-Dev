import React from "react";
import { getCardDisplayInfo } from "../presentation/cardDatabase.ts";
import { printedItemLabel } from "../presentation/tradeNames.ts";

/** A promissory note id is `alias:faction`; card text is keyed by the alias. */
function cardLabelFor(label: string): string {
  const m = label.match(/^(Promissory Note|Action Card|Secret Objective):\s*([^:]+)(?::.*)?$/);
  return m ? `${m[1]}: ${m[2].trim()}` : label;
}

/**
 * Render a trade item with its description (card text, amount, ...).
 * `kind` marks items that are cards, so a card with no known text says so instead of staying silent.
 */
export const TradeItemDisplay: React.FC<{
  label: string;
  description?: string;
  type: "offer" | "receive";
  isCard?: boolean;
  testId?: string;
}> = ({ label, description, type, isCard, testId }) => {
  const cardInfo = getCardDisplayInfo(cardLabelFor(label));
  const shown = printedItemLabel(label);
  const displayDesc = description || cardInfo?.description;

  return (
    <div className={`trade-item trade-item--${type}`} data-testid={testId}>
      <div className="trade-item__header">
        <span className="trade-item__name" title={shown !== label ? label : undefined}>
          {shown}
        </span>
        {cardInfo?.type && <span className="trade-item__badge">{cardInfo.type}</span>}
      </div>
      {displayDesc ? (
        <p className="trade-item__description">{displayDesc}</p>
      ) : (
        isCard && <p className="trade-item__description">Card text is not available in this view.</p>
      )}
      {cardInfo?.phase && (
        <div className="trade-item__meta">
          <span className="trade-item__meta-label">Phase:</span>
          <span>{cardInfo.phase}</span>
        </div>
      )}
    </div>
  );
};
