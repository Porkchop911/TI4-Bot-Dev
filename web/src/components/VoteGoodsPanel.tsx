import React, { useState } from "react";
import { voteGoodsLabel, type VoteGoodsView } from "../presentation/voteGoods.ts";
import "./DecisionContext.css";

/** Hacan's commander: pick how many trade goods to spend, see the votes before and after. */
export const VoteGoodsPanel: React.FC<{
  view: VoteGoodsView;
  disabled: boolean;
  onChoose: (optionId: string) => void;
}> = ({ view, disabled, onChoose }) => {
  const [n, setN] = useState(1);
  const amount = Math.min(Math.max(n, 1), view.goods);
  const option = view.spend.get(amount);
  return (
    <div className="secondary-panel offer-card" data-testid="vote-goods-panel">
      <div className="secondary-panel__card">
        <div className="secondary-panel__title">
          {view.title}
          <span className="secondary-panel__tag">commander</span>
        </div>
        {view.window && <div className="secondary-panel__meta">{view.window}</div>}
        {view.text && <p className="secondary-panel__text">{view.text}</p>}
      </div>
      <dl className="offer-card__facts">
        <div className="offer-card__fact">
          <dt>Voting for</dt>
          <dd>{view.outcome}</dd>
        </div>
        <div className="offer-card__fact">
          <dt>Votes so far</dt>
          <dd data-testid="vote-goods-votes">{view.votes}</dd>
        </div>
        <div className="offer-card__fact">
          <dt>Trade goods</dt>
          <dd>{view.goods}</dd>
        </div>
      </dl>
      <div className="vote-goods__stepper" role="group" aria-label="Trade goods to spend">
        <button
          type="button"
          className="button button--secondary"
          data-testid="vote-goods-less"
          disabled={disabled || amount <= 1}
          onClick={() => setN(amount - 1)}
        >
          −
        </button>
        <span className="vote-goods__amount" data-testid="vote-goods-amount">
          {amount} <small>of {view.goods}</small>
        </span>
        <button
          type="button"
          className="button button--secondary"
          data-testid="vote-goods-more"
          disabled={disabled || amount >= view.goods}
          onClick={() => setN(amount + 1)}
        >
          +
        </button>
      </div>
      <div className="secondary-panel__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="vote-goods-spend"
          disabled={disabled || !option}
          onClick={() => option && onChoose(option.id)}
        >
          {voteGoodsLabel(view, amount)}
        </button>
        <button
          type="button"
          className="button button--secondary"
          data-testid="vote-goods-skip"
          disabled={disabled}
          onClick={() => onChoose(view.decline.id)}
        >
          Don't spend any
        </button>
      </div>
    </div>
  );
};
