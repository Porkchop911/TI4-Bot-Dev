import React from "react";
import type { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describeUnitAbilityOption } from "../presentation/unitAbilityOptions.ts";
import { ValueText } from "./PlanetValueIcons.tsx";
import { UnitIcon } from "./UnitIcon.tsx";

/** Inside an option row: the unit's icon and where it comes from, goes to and what it costs. */
export const UnitAbilityOptionNote: React.FC<{
  choice: PendingChoiceDto;
  option: ChoiceOptionDto;
  board?: BoardView | null;
}> = ({ choice, option, board }) => {
  const info = describeUnitAbilityOption(choice, option, board);
  if (!info || (info.lines.length === 0 && !info.iconType)) return null;
  return (
    <div className="text-muted" data-testid="unit-ability-note" style={{ fontSize: 12 }}>
      {info.iconType && <UnitIcon type={info.iconType} size={16} />}
      {info.lines.map((line) => (
        <div key={line}>
          <ValueText text={line} />
        </div>
      ))}
    </div>
  );
};
