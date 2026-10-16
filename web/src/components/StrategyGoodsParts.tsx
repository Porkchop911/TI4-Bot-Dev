import React from "react";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { goodsNote, goodsOnCard, investmentsProgress } from "../presentation/strategyGoods.ts";
import { useDecisionTable } from "./PoliticsDecisionParts.tsx";

/** Above the card grid of Manipulate Investments: which good this is and what is still owed. */
export const InvestmentsContextPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const progress = investmentsProgress(choice);
  if (!progress) return null;
  return (
    <div className="politics-panel" data-testid="investments-panel">
      <div className="politics-panel__title">
        Manipulate Investments
        <span className="politics-panel__tag" data-testid="investments-step">
          trade good {progress.step} of {progress.of}
        </span>
      </div>
      <p className="text-muted">
        Place {progress.of} trade goods from the supply on strategy cards of your choice, on at least 3
        different cards.{" "}
        {progress.distinctOwed > 0
          ? `${progress.distinctOwed} more different card${progress.distinctOwed === 1 ? " is" : "s are"} still owed.`
          : "The three different cards are covered."}
      </p>
    </div>
  );
};

/** Under a strategy card option: the trade goods lying on it now. */
export const StrategyGoodsNote: React.FC<{ choice: PendingChoiceDto; option: ChoiceOptionDto }> = ({
  choice,
  option,
}) => {
  const table = useDecisionTable();
  const note = goodsNote(choice, table, option.id);
  if (!note) return null;
  return (
    <div className="politics-option-note" data-testid="strategy-goods-note">
      {note}
    </div>
  );
};

/** In the card's title row, so it stays in view however long the card text is: the goods on it. */
export const StrategyGoodsBadge: React.FC<{ option: ChoiceOptionDto }> = ({ option }) => {
  const table = useDecisionTable();
  const goods = goodsOnCard(table, option.id);
  if (goods === null) return null;
  return (
    <span
      className="strategy-goods-badge"
      data-testid="strategy-goods-badge"
      data-empty={goods === 0 ? "true" : undefined}
      title={`${goods} trade good${goods === 1 ? "" : "s"} on this card`}
    >
      {goods} TG
    </span>
  );
};
