import React, { useState } from "react";
import { BatchResume, canContinueBatch } from "../protocol/client.ts";
import { PendingChoiceDto } from "../protocol/types.ts";

export interface PausedPlanBannerProps {
  resume: BatchResume;
  choice: PendingChoiceDto | null;
  viewerSeat?: string | null;
  onContinue: () => Promise<void>;
  onDismiss: () => void;
}

const WHAT: Record<BatchResume["plan"]["kind"], string> = {
  tactical_movement: "movement plan",
  payment: "payment plan",
  agenda_vote_planets: "vote",
  production: "production plan",
  casualties: "hit assignment",
  tokens: "token plan",
  ready_planets: "planets to ready",
};

/**
 * Shown when the server stopped a confirmed plan at a reaction window. The steps before it are
 * done; the window is answered the normal way; the rest of the plan is sent again on request and
 * the server checks it against what the engine offers then.
 */
export const PausedPlanBanner: React.FC<PausedPlanBannerProps> = ({
  resume,
  choice,
  viewerSeat,
  onContinue,
  onDismiss,
}) => {
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const ready = canContinueBatch(resume, choice, viewerSeat);
  const left = resume.plan.steps.length;
  const what = WHAT[resume.plan.kind];
  return (
    <div className="paused-plan" role="status" data-testid="paused-plan">
      <span className="paused-plan__text">
        {resume.applied > 0
          ? `Your ${what} paused after ${resume.applied} step${resume.applied === 1 ? "" : "s"}: `
          : `Your ${what} paused: `}
        {ready
          ? `${left} step${left === 1 ? "" : "s"} still to do.`
          : resume.waiting.ownSeat
            ? "answer the reaction first."
            : "waiting for another player's reaction."}
        {error ? ` ${error}` : ""}
      </span>
      {ready && (
        <button
          type="button"
          className="paused-plan__continue"
          data-testid="paused-plan-continue"
          disabled={running}
          onClick={() => {
            setRunning(true);
            setError(null);
            onContinue()
              .catch((e: unknown) => setError(e instanceof Error ? e.message : String(e)))
              .finally(() => setRunning(false));
          }}
        >
          Continue plan
        </button>
      )}
      <button type="button" className="paused-plan__dismiss"
        data-testid="paused-plan-dismiss" onClick={onDismiss}>
        Dismiss
      </button>
    </div>
  );
};
