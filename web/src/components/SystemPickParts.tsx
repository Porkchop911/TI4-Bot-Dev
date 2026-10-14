import React, { useState } from "react";
import { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import { isSystemPickChoice, systemPickOptionSystem } from "../presentation/systemFacts.ts";
import { describeSystem } from "../presentation/systemFacts.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { SystemFactsView } from "./SystemFactsView.tsx";
import "./SystemFacts.css";

/** Facts for one option row of a system pick (nothing for any other decision). */
export const SystemPickOptionFacts: React.FC<{
  choice: PendingChoiceDto;
  optionId: string;
  board?: BoardView | null;
}> = ({ choice, optionId, board }) => {
  if (!isSystemPickChoice(choice, board)) return null;
  const option = choice.options.find((o) => o.id === optionId);
  const systemId = option ? systemPickOptionSystem(option) : null;
  return systemId ? <SystemFactsView systemId={systemId} board={board} /> : null;
};

/** Hides the list so the highlighted systems on the map can be clicked instead. */
export const SystemPickMapButton: React.FC<{
  choice: PendingChoiceDto;
  board?: BoardView | null;
  onMinimize: () => void;
}> = ({ choice, board, onMinimize }) =>
  isSystemPickChoice(choice, board) ? (
    <button
      type="button"
      className="button button--secondary"
      data-testid="system-pick-inspect-map-btn"
      onClick={onMinimize}
    >
      Choose on the map
    </button>
  ) : null;

/** The confirm bar shown while the list is minimized: the selected system and a confirm button. */
export const SystemPickConfirmBar: React.FC<{
  choice: PendingChoiceDto;
  board?: BoardView | null;
  selectedOptionId?: string;
  onSubmit: (optionId: string) => Promise<void>;
}> = ({ choice, board, selectedOptionId, onSubmit }) => {
  const present = useParticipantText();
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  if (!isSystemPickChoice(choice, board)) return null;
  const option = choice.options.find((o) => o.id === selectedOptionId);
  const systemId = option ? systemPickOptionSystem(option) : null;
  if (!option || !systemId) {
    return (
      <span className="text-muted" data-testid="system-pick-hint">
        Click a highlighted system on the map
      </span>
    );
  }
  const confirm = async () => {
    if (busy) return;
    setBusy(true);
    setError(null);
    try {
      await onSubmit(option.id);
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setBusy(false);
    }
  };
  return (
    <div className="system-pick-bar" data-testid="system-pick-bar">
      <span data-testid="system-pick-selected">
        {present(choice.prompt)}: <strong>{describeSystem(systemId, board).title}</strong>
      </span>
      <button
        type="button"
        className="button button--primary"
        data-testid="confirm-activation-btn"
        disabled={busy}
        onClick={() => void confirm()}
      >
        {busy ? "Submitting..." : "Confirm"}
      </button>
      {error && (
        <span role="alert" className="text-danger">
          {error}
        </span>
      )}
    </div>
  );
};
