import React from "react";
import { ValueText } from "./PlanetValueIcons.tsx";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describePrediction, describeRider } from "../presentation/predictOutcome.ts";
import { useDecisionTable } from "./PoliticsDecisionParts.tsx";

/** Above the options: the rider as printed, and that a prediction is aloud and pays only when right. */
export const PredictOutcomePanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const rider = describeRider(choice);
  if (!rider) return null;
  return (
    <div className="politics-panel" data-testid="predict-outcome-panel">
      <div className="politics-panel__title">
        {rider.name} <span className="politics-panel__tag">predict an outcome</span>
      </div>
      {rider.text && (
        <p className="politics-panel__text" data-testid="predict-outcome-text">
          {rider.text}
        </p>
      )}
    </div>
  );
};

/** Inside an option row: the outcome in words, not as an engine id. */
export const PredictOutcomeNote: React.FC<{ choice: PendingChoiceDto; option: ChoiceOptionDto }> = ({
  choice,
  option,
}) => {
  const table = useDecisionTable();
  const outcome = describePrediction(choice, option, table);
  if (!outcome) return null;
  return (
    <div className="politics-option-note" data-testid="predict-outcome-note">
      Predicted outcome: <strong>
        <ValueText text={outcome} />
      </strong>
    </div>
  );
};
