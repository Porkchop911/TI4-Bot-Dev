import React from "react";
import { ValueText } from "./PlanetValueIcons.tsx";
import type { BoardView, ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { describeLegendaryOption, legendaryWindow } from "../presentation/legendaryMenu.ts";

/** Above the options: when the window is open and what choosing does to the card. */
export const LegendaryContextPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const when = legendaryWindow(choice);
  if (!when) return null;
  return (
    <div className="politics-panel" data-testid="legendary-panel">
      <div className="politics-panel__title">
        Legendary planet abilities <span className="politics-panel__tag">{when}</span>
      </div>
      <p className="text-muted">
        Using an ability exhausts its card until the status phase. You are asked again for any other
        ability that is still ready.
      </p>
    </div>
  );
};

/** Inside an option row: the planet that carries the ability, and the card's printed text. */
export const LegendaryOptionNote: React.FC<{
  choice: PendingChoiceDto;
  option: ChoiceOptionDto;
  board?: BoardView | null;
}> = ({ choice, option, board }) => {
  const view = describeLegendaryOption(choice, option, board);
  if (!view) return null;
  return (
    <div className="politics-option-note" data-testid="legendary-option-note">
      <strong>{view.planet}</strong>
      {view.stats && <span className="text-muted"> (<ValueText text={view.stats} />)</span>}
      {view.systemId && <span className="text-muted"> · system {view.systemId}</span>}
      {view.text && (
        <p className="politics-panel__text" data-testid="legendary-option-text">
          {view.text}
        </p>
      )}
    </div>
  );
};
