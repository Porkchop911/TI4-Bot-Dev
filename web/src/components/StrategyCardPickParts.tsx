import React from "react";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describeStrategyCardPick, strategyCardHolder } from "../presentation/strategyCardPick.ts";
import { SeatTag } from "./SeatTag.tsx";
import "./ActionCardPickers.css";

/** Above the strategy-card grid of Public Disgrace, Overrule and Strategize: the card and what it does. */
export const StrategyCardPickPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const pick = describeStrategyCardPick(choice);
  if (!pick) return null;
  return (
    <div className="politics-panel" data-testid="strategy-card-pick-panel">
      <div className="politics-panel__title">
        {pick.cardName}
        <span className="politics-panel__tag">
          {pick.half ? `perform the ${pick.half} ability` : "choose a different card"}
        </span>
      </div>
      {pick.playedBy && (
        <p className="politics-panel__text" data-testid="strategy-card-pick-by">
          <SeatTag seat={pick.playedBy} /> played it on your choice
          {pick.takenCard ? (
            <>
              : <strong>{pick.takenCard}</strong> went back to the mat.
            </>
          ) : (
            "."
          )}
        </p>
      )}
      <p className="text-muted">{pick.summary}</p>
    </div>
  );
};

/** In a card's title row: who holds it (Overrule and Strategize), or that nobody has chosen it. */
export const StrategyCardHolderTag: React.FC<{ choice: PendingChoiceDto; option: ChoiceOptionDto }> = ({
  choice,
  option,
}) => {
  const holder = strategyCardHolder(choice, option);
  if (holder === undefined) return null;
  return (
    <div className="strategy-holder" data-testid="strategy-card-holder">
      {holder === null ? (
        <span className="strategy-holder__unchosen">Unchosen</span>
      ) : (
        <>
          Held by <SeatTag seat={holder} />
        </>
      )}
    </div>
  );
};
