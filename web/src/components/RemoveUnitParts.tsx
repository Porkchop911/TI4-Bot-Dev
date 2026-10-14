import React from "react";
import type { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describeRemoveUnit } from "../presentation/removeUnit.ts";
import { UnitIcon } from "./UnitIcon.tsx";

/** Above the options of "remove a unit": how far over, in which system, and what stands there. */
export const RemoveUnitPanel: React.FC<{
  choice: PendingChoiceDto;
  board?: BoardView | null;
}> = ({ choice, board }) => {
  const view = describeRemoveUnit(choice, board);
  if (!view) return null;
  return (
    <div className="politics-panel" data-testid="remove-unit-panel">
      <div className="politics-panel__title" data-testid="remove-unit-headline">
        {view.headline}
      </div>
      {view.present && (
        <p className="text-muted" data-testid="remove-unit-present">
          Your forces in that space area: {view.present}
        </p>
      )}
      <p className="text-muted">Choose which unit to remove.</p>
    </div>
  );
};

/** Inside an option row: the unit's icon, where it stands and what removing it costs. */
export const RemoveUnitOptionNote: React.FC<{
  choice: PendingChoiceDto;
  option: Pick<ChoiceOptionDto, "id" | "label">;
  board?: BoardView | null;
}> = ({ choice, option, board }) => {
  const info = describeRemoveUnit(choice, board)?.option(option);
  if (!info) return null;
  return (
    <div className="text-muted" data-testid="remove-unit-note" style={{ fontSize: 12 }}>
      <UnitIcon type={info.iconType} size={16} /> {info.where}
      {info.effect && <div>{info.effect}</div>}
    </div>
  );
};
