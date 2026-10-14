import React, { useState } from "react";
import { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";

export interface SystemActivationBarProps {
  choice: PendingChoiceDto;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  selectedOptionId?: string;
  selectedSystemId?: string | null;
  onSelectOption?: (optionId: string) => void;
  onSubmit: (optionId: string) => Promise<void>;
  boardView?: BoardView;
  lastError?: string | null;
}

export const SystemActivationBar: React.FC<SystemActivationBarProps> = ({
  choice,
  viewerSeat,
  selectedOptionId,
  selectedSystemId,
  onSelectOption,
  onSubmit,
  boardView,
  lastError,
}) => {
  const present = useParticipantText();
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  const isActor = Boolean(choice && viewerSeat && choice.actor === viewerSeat);

  if (!isActor) {
    return (
      <aside
        data-testid="system-activation-bar"
        className="choice-banner panel system-activation-bar"
        aria-label="System activation status"
      >
        <div className="system-activation-bar__body">
          <span className="system-activation-bar__hint text-muted">
            Waiting for {present(choice.actor)} to activate a system...
          </span>
        </div>
      </aside>
    );
  }

  const selectedOption = choice.options.find((o) => o.id === selectedOptionId);
  const targetSystemId =
    (selectedOption?.payload?.system as string) ||
    (selectedOption?.id.startsWith("activate|")
      ? selectedOption.id.replace("activate|", "")
      : selectedOption?.id);

  const targetMatchesSelectedSystem = !selectedSystemId || targetSystemId === selectedSystemId;
  const activeOption = targetMatchesSelectedSystem ? selectedOption : null;

  const targetTile = boardView?.map_tiles?.find((t) => t.system_id === targetSystemId);
  const targetLabel =
    targetTile?.label ||
    (targetSystemId === "18" ? "Mecatol Rex" : targetSystemId ? `#${targetSystemId}` : "");

  const inspectedTile = selectedSystemId
    ? boardView?.map_tiles?.find((t) => t.system_id === selectedSystemId)
    : null;
  const inspectedSys = selectedSystemId ? boardView?.systems?.[selectedSystemId] : null;
  const hasViewerToken = Boolean(inspectedSys?.command_tokens?.some((ct) => ct === viewerSeat));
  const inspectedLabel =
    inspectedTile?.label ||
    (selectedSystemId === "18" ? "Mecatol Rex" : selectedSystemId ? `#${selectedSystemId}` : "");

  const handleConfirm = async () => {
    if (!activeOption || isSubmitting) return;
    setIsSubmitting(true);
    setSubmissionError(null);
    try {
      await onSubmit(activeOption.id);
    } catch (err: unknown) {
      setSubmissionError(err instanceof Error ? err.message : String(err));
    } finally {
      setIsSubmitting(false);
    }
  };

  const handleCancel = () => {
    onSelectOption?.("");
    setSubmissionError(null);
  };

  const errorMessage = submissionError || lastError;

  return (
    <aside
      data-testid="system-activation-bar"
      className="choice-banner panel system-activation-bar"
      aria-label="System activation"
    >
      <div className="system-activation-bar__body">
        {activeOption ? (
          <div className="system-activation-bar__confirm-row">
            <span className="system-activation-bar__title">
              Activate <strong>{targetLabel}</strong> {targetSystemId ? `(#${targetSystemId})` : ""}
            </span>
            <div className="system-activation-bar__actions">
              <button
                type="button"
                className="button button--primary"
                data-testid="confirm-activation-btn"
                disabled={isSubmitting}
                onClick={handleConfirm}
              >
                {isSubmitting ? "Activating..." : "Confirm Activation"}
              </button>
              <button
                type="button"
                className="button button--secondary"
                data-testid="cancel-activation-btn"
                disabled={isSubmitting}
                onClick={handleCancel}
              >
                Cancel
              </button>
            </div>
          </div>
        ) : selectedSystemId ? (
          <div className="system-activation-bar__prompt-row">
            {hasViewerToken ? (
              <>
                <span className="badge badge--warning">Activated / Blocked</span>
                <span className="system-activation-bar__prompt">
                  <strong>
                    {inspectedLabel} (#{selectedSystemId})
                  </strong>{" "}
                  already contains your command token
                </span>
              </>
            ) : (
              <>
                <span className="badge badge--secondary">Not Targetable</span>
                <span className="system-activation-bar__prompt">
                  <strong>
                    {inspectedLabel} (#{selectedSystemId})
                  </strong>{" "}
                  cannot be activated in this action
                </span>
              </>
            )}
            <span className="system-activation-bar__hint text-muted">
              (Click a highlighted system hex to activate)
            </span>
          </div>
        ) : (
          <div className="system-activation-bar__prompt-row">
            <span className="badge badge--primary">Tactical Action</span>
            <span className="system-activation-bar__prompt">
              {present(choice.prompt || "Select a system on the map to activate")}
            </span>
            <span className="system-activation-bar__hint text-muted">
              (Click a highlighted system hex)
            </span>
          </div>
        )}
        {errorMessage && (
          <div
            role="alert"
            className="system-activation-bar__error text-danger"
            data-testid="activation-error"
          >
            {errorMessage}
          </div>
        )}
      </div>
    </aside>
  );
};
