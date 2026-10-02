import React, { useState, useEffect, useMemo } from "react";
import { PendingChoiceDto } from "../protocol/types.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";

export interface ReactionStatusBarProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose?: () => void;
  lastError?: string | null;
  autoPassTimeoutSeconds?: number;
}

export const ReactionStatusBar: React.FC<ReactionStatusBarProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isOpen,
  onClose,
  lastError,
  autoPassTimeoutSeconds,
}) => {
  const display = usePlayerIdentity();
  const isActor = Boolean(choice && viewerSeat && choice.actor === viewerSeat);
  const [isSubmitting, setIsSubmitting] = useState(false);
  const [secondsRemaining, setSecondsRemaining] = useState<number | null>(
    autoPassTimeoutSeconds ?? null,
  );
  const [isPinned, setIsPinned] = useState(false);
  const [submissionError, setSubmissionError] = useState<string | null>(null);

  // Reset state on nonce change
  useEffect(() => {
    setIsSubmitting(false);
    setSecondsRemaining(autoPassTimeoutSeconds ?? null);
    setSubmissionError(null);
  }, [choice?.nonce, autoPassTimeoutSeconds]);

  // Use model.declineOption when available (centralized extraction), fallback to local search
  const declineOption = useMemo(() => {
    return (
      model?.declineOption ??
      choice?.options.find((o) => o.id === "decline" || o.kind === "decline") ??
      null
    );
  }, [choice, model]);

  const reactionOptions = useMemo(() => {
    if (!choice) return [];
    return choice.options.filter((o) => o.id !== "decline" && o.kind !== "decline");
  }, [choice]);

  const handleAction = async (optionId: string) => {
    if (isSubmitting) return;
    setIsSubmitting(true);
    try {
      await onSubmit(optionId);
    } catch (error) {
      setSubmissionError(error instanceof Error ? error.message : String(error));
    } finally {
      setIsSubmitting(false);
    }
  };

  // Keyboard navigation (Spacebar -> Pass, Enter -> Play Reaction)
  useEffect(() => {
    if (!isOpen || !choice || !isActor || isSubmitting) return;

    const handleKeyDown = (e: KeyboardEvent) => {
      // Ignore if focused on an input element
      const activeTag = document.activeElement?.tagName?.toLowerCase();
      if (activeTag === "input" || activeTag === "textarea" || activeTag === "select") {
        return;
      }

      if (e.key === " " || e.code === "Space") {
        if (declineOption) {
          e.preventDefault();
          handleAction(declineOption.id);
        }
      } else if (e.key === "Enter") {
        if (reactionOptions.length > 0) {
          e.preventDefault();
          handleAction(reactionOptions[0].id);
        }
      }
    };

    window.addEventListener("keydown", handleKeyDown);
    return () => window.removeEventListener("keydown", handleKeyDown);
  }, [isOpen, choice, isActor, isSubmitting, declineOption, reactionOptions]);

  // Countdown timer for auto-pass (if configured and not pinned)
  useEffect(() => {
    if (!isOpen || !choice || !isActor || isPinned || secondsRemaining === null || isSubmitting) {
      return;
    }

    if (secondsRemaining <= 0) {
      if (declineOption) {
        // Clear the countdown immediately before calling handleAction so the
        // effect cannot re-fire if isSubmitting changes while the network
        // call is in flight.
        setSecondsRemaining(null);
        handleAction(declineOption.id);
      }
      return;
    }

    const timer = setTimeout(() => {
      setSecondsRemaining((prev) => (prev !== null ? prev - 1 : null));
    }, 1000);

    return () => clearTimeout(timer);
  }, [isOpen, choice, isActor, isPinned, secondsRemaining, isSubmitting, declineOption]);

  if (!isOpen || !choice) return null;

  return (
    <div
      role="region"
      aria-label="Reaction Window"
      data-testid="reaction-status-bar"
      className="reaction-status-bar"
    >
      <DecisionHeader
        actor={choice.actor}
        title="Respond to the action card"
        instruction={choice.prompt}
        progress={
          secondsRemaining === null || isPinned
            ? undefined
            : `${secondsRemaining} seconds remaining`
        }
        onMinimize={() => onClose?.()}
      />
      {/* Icon & Details */}
      <div className="reaction-status-bar__details">
        <span className="reaction-status-bar__icon" role="img" aria-label="Reaction opportunity">
          ⚡
        </span>
        <div className="reaction-status-bar__text">
          <div data-testid="reaction-bar-title" className="reaction-status-bar__title">
            Reaction Opportunity
            {secondsRemaining !== null && !isPinned && (
              <span className="reaction-status-bar__countdown">({secondsRemaining}s)</span>
            )}
          </div>
          <div data-testid="reaction-bar-prompt" className="reaction-status-bar__prompt">
            {choice.prompt}
          </div>
        </div>
      </div>

      {/* Spectator Notice */}
      {!isActor ? (
        <div data-testid="spectator-reaction-notice" className="reaction-status-bar__spectator">
          Waiting for {display(choice.actor).label}...
        </div>
      ) : (
        /* Action Buttons */
        <div className="reaction-status-bar__actions">
          {/* Reaction Cards */}
          {reactionOptions.map((opt) => (
            <button
              key={opt.id}
              type="button"
              data-testid={`play-reaction-btn-${opt.id}`}
              onClick={() => handleAction(opt.id)}
              disabled={isSubmitting}
              className="button button--primary reaction-status-bar__play"
            >
              Play {opt.label}
            </button>
          ))}

          {/* Pass Button */}
          {declineOption && (
            <button
              type="button"
              data-testid="pass-reaction-btn"
              onClick={() => handleAction(declineOption.id)}
              disabled={isSubmitting}
              className="button button--secondary reaction-status-bar__pass"
            >
              Pass (Spacebar)
            </button>
          )}

          {/* Pinned Toggle */}
          <label className="reaction-status-bar__pin">
            <input
              type="checkbox"
              data-testid="pin-reaction-toggle"
              checked={isPinned}
              onChange={(e) => setIsPinned(e.target.checked)}
            />
            Pin
          </label>
        </div>
      )}

      {/* Error alert if present */}
      {(lastError || submissionError) && (
        <div data-testid="reaction-error-badge" role="alert" className="reaction-status-bar__error">
          {submissionError || lastError}
        </div>
      )}
    </div>
  );
};
