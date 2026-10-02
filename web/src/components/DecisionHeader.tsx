import React from "react";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";

/** Identical header/minimize semantics for both modal and board-side decisions. */
export const DecisionHeader: React.FC<{
  title: string;
  instruction?: string;
  actor?: string;
  progress?: string;
  onMinimize: () => void;
  titleTestId?: string;
  minimizeTestId?: string;
  minimizeLabel?: string;
  minimizeIcon?: string;
}> = ({
  title,
  instruction,
  actor,
  progress,
  onMinimize,
  titleTestId,
  minimizeTestId,
  minimizeLabel = "Minimize decision",
  minimizeIcon = "−",
}) => {
  const display = usePlayerIdentity();
  const participant = display(actor);
  return (
    <header className="decision-frame__header">
      <div>
        {actor && participant.position != null && <small>{participant.label}</small>}
        <h2 data-testid={titleTestId}>{title}</h2>
        {instruction && instruction !== title && <p className="text-muted">{instruction}</p>}
        {progress && <p className="text-muted">{progress}</p>}
      </div>
      <button
        type="button"
        data-testid={minimizeTestId}
        className="button button--secondary button--icon"
        aria-label={minimizeLabel}
        title={minimizeLabel}
        onClick={onMinimize}
      >
        {minimizeIcon}
      </button>
    </header>
  );
};
