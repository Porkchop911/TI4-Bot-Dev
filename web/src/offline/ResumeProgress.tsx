import React from "react";
import { durationLabel, progressFraction } from "./setup.ts";
import type { OfflineProgress } from "./types.ts";

const TITLE: Record<OfflineProgress["phase"], string> = {
  "loading-engine": "Loading the game engine",
  replaying: "Restoring your game",
  starting: "Setting up the table",
};

/**
 * Progress bar for engine load and replay. Replay of a full game takes up to about 14 s; the
 * bar is determinate when the total step count is known, otherwise it pulses.
 */
export const ResumeProgress: React.FC<{
  progress: OfflineProgress;
  estimateMs?: number | null;
  onCancel?: () => void;
}> = ({ progress, estimateMs = null, onCancel }) => {
  const fraction = progressFraction(progress.done, progress.total);
  const percent = fraction === null ? null : Math.round(fraction * 100);
  return (
    <section className="panel offline-progress" data-testid="offline-progress" aria-live="polite">
      <h2>{TITLE[progress.phase]}</h2>
      <div
        className={`offline-bar${fraction === null ? " offline-bar--indeterminate" : ""}`}
        role="progressbar"
        aria-label={TITLE[progress.phase]}
        aria-valuemin={0}
        aria-valuemax={100}
        aria-valuenow={percent ?? undefined}
      >
        <div className="offline-bar__fill" style={fraction === null ? undefined : { width: `${percent}%` }} />
      </div>
      <p className="text-muted" data-testid="offline-progress-text">
        {progress.phase === "replaying" && progress.total !== null
          ? `Replaying move ${progress.done} of ${progress.total} (${percent}%).`
          : "One moment."}
        {progress.phase === "replaying" && ` A long game can take ${durationLabel(estimateMs ?? 14000)}.`}
      </p>
      {onCancel && (
        <button type="button" className="button button--outline" data-testid="offline-cancel" onClick={onCancel}>
          Cancel
        </button>
      )}
    </section>
  );
};
