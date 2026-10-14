import React from "react";
import { offerUnitName, type OfferCardView } from "../presentation/offerCard.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { UnitIcon } from "./UnitIcon.tsx";
import "./DecisionContext.css";

/**
 * A decision as a named card: what is asked, the facts that matter (unit, place, amounts before
 * and after) and one button per answer that says what it does.
 */
export const OfferCardPanel: React.FC<{
  view: OfferCardView;
  disabled: boolean;
  onChoose: (optionId: string) => void;
}> = ({ view, disabled, onChoose }) => {
  const display = usePlayerIdentity();
  return (
  <div className="secondary-panel offer-card" data-testid="offer-card-panel">
    <div className="secondary-panel__card">
      <div className="secondary-panel__title">
        {view.title}
        <span className="secondary-panel__tag">{view.tag}</span>
      </div>
      {view.window && (
        <div className="secondary-panel__meta" data-testid="offer-card-window">
          {view.window}
        </div>
      )}
      {view.text && (
        <p className="secondary-panel__text" data-testid="offer-card-text">
          {view.text}
        </p>
      )}
    </div>
    {view.facts.length > 0 && (
      <dl className="offer-card__facts" data-testid="offer-card-facts">
        {view.facts.map((fact) => (
          <div key={fact.label} className="offer-card__fact">
            <dt>{fact.label}</dt>
            <dd>
              {fact.unit && (
                <>
                  <UnitIcon type={fact.unit} size={16} /> {offerUnitName(fact.unit)}
                </>
              )}
              {fact.seat && display(fact.seat).label}
              {fact.change && (
                <>
                  {fact.change.from} → <strong>{fact.change.to}</strong>
                  {fact.change.of !== null && (
                    <span className="text-muted"> of {fact.change.of}</span>
                  )}
                </>
              )}
              {fact.text}
            </dd>
          </div>
        ))}
      </dl>
    )}
    <div className="secondary-panel__actions offer-card__answers">
      {view.answers.map((answer) => (
        <button
          key={answer.option.id}
          type="button"
          className={`button ${answer.isDecline ? "button--secondary" : "button--primary"}`}
          data-testid={`offer-card-answer-${answer.option.id}`}
          disabled={disabled}
          onClick={() => onChoose(answer.option.id)}
        >
          <span>{answer.label}</span>
          {(answer.hint || answer.seat) && (
            <small className="offer-card__hint">
              {answer.seat && <>{display(answer.seat).label}{answer.hint ? " · " : ""}</>}
              {answer.hint}
            </small>
          )}
        </button>
      ))}
    </div>
  </div>
);
};
