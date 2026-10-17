import React, { useId, useState } from "react";
import type { NumberStepperView } from "../presentation/numberStepper.ts";
import "./ActionCardPickers.css";

/** A quantity to step with the plus and minus keys or the keyboard, and the words for what it buys. */
export const NumberStepperPanel: React.FC<{
  view: NumberStepperView;
  disabled?: boolean;
  onConfirm: (optionId: string) => void;
}> = ({ view, disabled, onConfirm }) => {
  const [count, setCount] = useState(view.min);
  const inputId = useId();
  const clamp = (n: number) => Math.min(view.max, Math.max(view.min, Math.round(Number.isFinite(n) ? n : view.min)));
  const option = view.optionFor(count);
  return (
    <section className="stepper" data-testid="number-stepper" aria-label={`${view.cardName}: how many`}>
      <div className="politics-panel">
        <div className="politics-panel__title">
          {view.cardName}
          <span className="politics-panel__tag">choose an amount</span>
        </div>
        <p className="text-muted">You can spend up to {view.max}.</p>
      </div>
      <div className="stepper__control">
        <button
          type="button"
          className="button button--secondary stepper__button"
          data-testid="stepper-minus"
          aria-label="One less"
          disabled={disabled || count <= view.min}
          onClick={() => setCount(clamp(count - 1))}
        >
          −
        </button>
        <label htmlFor={inputId} className="visually-hidden">
          Amount
        </label>
        <input
          id={inputId}
          className="input stepper__input"
          data-testid="stepper-input"
          type="number"
          inputMode="numeric"
          min={view.min}
          max={view.max}
          value={count}
          disabled={disabled}
          onChange={(event) => setCount(clamp(event.target.valueAsNumber))}
        />
        <button
          type="button"
          className="button button--secondary stepper__button"
          data-testid="stepper-plus"
          aria-label="One more"
          disabled={disabled || count >= view.max}
          onClick={() => setCount(clamp(count + 1))}
        >
          +
        </button>
        <button
          type="button"
          className="button button--secondary"
          data-testid="stepper-max"
          disabled={disabled || count === view.max}
          onClick={() => setCount(view.max)}
        >
          All ({view.max})
        </button>
      </div>
      <p className="stepper__summary" data-testid="stepper-summary" aria-live="polite">
        {view.describe(count)}
      </p>
      <div className="stepper__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="stepper-confirm"
          disabled={disabled || !option}
          onClick={() => option && onConfirm(option.id)}
        >
          {count === 0 ? "Spend nothing" : `Spend ${count}`}
        </button>
      </div>
    </section>
  );
};
