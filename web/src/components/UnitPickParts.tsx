import React from "react";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describeUnitPickCard, describeUnitPickOption } from "../presentation/unitPick.ts";
import { UnitIcon } from "./UnitIcon.tsx";
import { MechInfoRow, UnitInfoButton } from "./UnitInfo.tsx";

/** Above the options of Refit Troops or Scuttle: the printed card. */
export const UnitPickPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const card = describeUnitPickCard(choice);
  if (!card) return null;
  return (
    <div className="politics-panel" data-testid="unit-pick-panel">
      <div className="politics-panel__title">
        {card.name}
        <span className="politics-panel__tag">pick a unit</span>
      </div>
      {card.text && (
        <p className="politics-panel__text" data-testid="unit-pick-card-text">
          {card.text}
        </p>
      )}
    </div>
  );
};

/** Inside an option row: the unit's icon, where it stands and what the card does to it. */
export const UnitPickOptionNote: React.FC<{ choice: PendingChoiceDto; option: ChoiceOptionDto }> = ({
  choice,
  option,
}) => {
  const info = describeUnitPickOption(choice, option);
  if (!info) return null;
  return (
    <div className="text-muted" data-testid="unit-pick-note" style={{ fontSize: 12 }}>
      <UnitIcon type={info.iconType} size={16} /> <strong>{info.title}</strong>{" "}
      <UnitInfoButton unit={info.iconType} name={info.title} /> · {info.where}
      <div>{info.effect}</div>
      {choice.context?.subtype?.endsWith("_pick_infantry") && (
        <MechInfoRow />
      )}
    </div>
  );
};
