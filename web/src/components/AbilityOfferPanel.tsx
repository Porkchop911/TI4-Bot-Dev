import React from "react";
import { abilityOfferLabel, type AbilityOfferView } from "../presentation/abilityOffer.ts";
import "./DecisionContext.css";

/** A paid faction ability as a named offer: the card, the price against what you hold, Skip. */
export const AbilityOfferPanel: React.FC<{
  view: AbilityOfferView;
  disabled: boolean;
  onChoose: (optionId: string) => void;
}> = ({ view, disabled, onChoose }) => {
  const cantPay = view.held < view.tradeGoods;
  return (
    <div className="secondary-panel" data-testid="ability-offer-panel">
      <div className="secondary-panel__card">
        <div className="secondary-panel__title">
          {view.name}
          <span className="secondary-panel__tag">faction ability</span>
        </div>
        {view.window && (
          <div className="secondary-panel__meta" data-testid="ability-offer-window">
            {view.window}
          </div>
        )}
        {view.effect && (
          <p className="secondary-panel__text" data-testid="ability-offer-effect">
            {view.effect}
          </p>
        )}
      </div>
      {view.dice.length > 0 && (
        <div className="secondary-panel__tokens" data-testid="ability-offer-dice">
          <span>Your dice</span>
          <strong>
            {view.dice.map((die, index) => (
              <span
                key={index}
                data-testid={die.hit ? "ability-offer-die-hit" : "ability-offer-die-miss"}
                style={{ marginLeft: 8, opacity: die.hit ? 1 : 0.6 }}
              >
                {die.face}
                {die.hit === null ? "" : die.hit ? " hit" : " miss"}
              </span>
            ))}
          </strong>
        </div>
      )}
      <div className="secondary-panel__tokens" data-testid="ability-offer-goods">
        <span>Trade goods</span>
        <strong>
          {view.held} ({view.held - view.tradeGoods} after)
        </strong>
      </div>
      <div className="secondary-panel__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="ability-offer-yes-btn"
          disabled={disabled || cantPay}
          onClick={() => onChoose(view.yes.id)}
        >
          {abilityOfferLabel(view)}
        </button>
        {view.alternative && (
          <button
            type="button"
            className="button button--secondary"
            data-testid="ability-offer-alt-btn"
            disabled={disabled || cantPay}
            onClick={() => onChoose(view.alternative!.id)}
          >
            {view.alternative.label}
          </button>
        )}
        <button
          type="button"
          className="button button--secondary"
          data-testid="ability-offer-skip-btn"
          disabled={disabled}
          onClick={() => onChoose(view.no.id)}
        >
          Don't use it
        </button>
      </div>
    </div>
  );
};
