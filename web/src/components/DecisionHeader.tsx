import React from "react";
import type { PendingChoiceDto } from "../protocol/types.ts";
import { decisionBreakthrough, describeDecisionHeader } from "../presentation/decisionSource.ts";
import { synergyText } from "../presentation/breakthroughs.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import "./DecisionContext.css";

/** Identical header/minimize semantics for both modal and board-side decisions. */
export const DecisionHeader: React.FC<{
  title: React.ReactNode;
  instruction?: string;
  actor?: string;
  progress?: React.ReactNode;
  /** When given, the header also shows what this decision is about and when it was asked. */
  choice?: Pick<PendingChoiceDto, "context"> | null;
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
  choice,
  onMinimize,
  titleTestId,
  minimizeTestId,
  minimizeLabel = "Minimize decision",
  minimizeIcon = "−",
}) => {
  const display = usePlayerIdentity();
  const participant = display(actor);
  const info = choice ? describeDecisionHeader(choice) : null;
  const breakthrough = choice ? decisionBreakthrough(choice) : undefined;
  return (
    <header className="decision-frame__header">
      <div>
        {info?.eyebrow && (
          <small className="decision-context__eyebrow" data-testid="decision-eyebrow">
            {info.eyebrow}
          </small>
        )}
        {actor && participant.position != null && <small>{participant.label}</small>}
        <h2 data-testid={titleTestId}>{title}</h2>
        {instruction && instruction !== title && <p className="text-muted">{instruction}</p>}
        {progress && <p className="text-muted">{progress}</p>}
        {breakthrough && (
          <div className="decision-context__card" data-testid="decision-breakthrough">
            <strong>Breakthrough: {breakthrough.name}</strong>{" "}
            <span className="text-muted">(synergy {synergyText(breakthrough)})</span>
            {breakthrough.text && <p className="decision-context__card-text">{breakthrough.text}</p>}
          </div>
        )}
        {info && info.chips.length > 0 && (
          <ul className="decision-context__chips" data-testid="decision-context-strip">
            {info.chips.map((chip) => (
              <li key={chip} className="decision-context__chip">
                {chip}
              </li>
            ))}
          </ul>
        )}
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
