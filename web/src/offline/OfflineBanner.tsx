import React from "react";
import { unavailableFeatures } from "./capabilities.ts";
import { engineLabel, useLoadedEngine } from "./engineInfo.ts";
import { StorageNotice } from "./StorageNotice.tsx";
import { feedbackChipText, useFeedbackQueue } from "./useFeedbackQueue.ts";
import type { Capabilities, OfflineFeedbackQueue, OfflineStorage } from "./types.ts";

/**
 * Slim strip shown above an offline game: says the game is local, lists what is not available
 * (driven by the capability flags, so it never lies about a feature that gets added later) and
 * shows the queued-feedback state.
 */
export const OfflineBanner: React.FC<{
  capabilities: Capabilities;
  feedback: OfflineFeedbackQueue | null;
  onRetryFeedback?: () => void;
  /** The save store: its warning (no persistent storage) is shown under the strip. */
  storage?: OfflineStorage;
  /** The game is open to look at only. */
  viewOnly?: boolean;
}> = ({ capabilities, feedback, onRetryFeedback, storage, viewOnly }) => {
  const missing = unavailableFeatures(capabilities).filter((x) => x !== "Instant feedback delivery");
  const items = useFeedbackQueue(feedback);
  const chip = feedbackChipText(items);
  // "Retry" sends every report that gave up (the sheet of an offline game has no other way to).
  const retry =
    onRetryFeedback ??
    (() => items.filter((item) => item.state === "failed").forEach((item) => feedback?.retry(item.localId)));
  const engine = engineLabel(useLoadedEngine());
  return (
    <div className="offline-banner-stack">
    <div className="offline-banner" role="status" data-testid="offline-banner">
      <strong className="offline-banner__tag">Offline game</strong>
      <span className="offline-banner__text">Runs on this device against rule bots.</span>
      {viewOnly && (
        <strong className="offline-banner__viewonly" data-testid="offline-view-only">
          View only
        </strong>
      )}
      {engine && (
        <span className="offline-banner__engine" data-testid="offline-engine" title="The engine file this page runs">
          {engine}
        </span>
      )}
      {missing.length > 0 && (
        <span className="offline-banner__missing" data-testid="offline-missing">
          Not available offline: {missing.join(", ")}.
        </span>
      )}
      {chip && (
        <span className="offline-banner__queue" data-testid="offline-feedback-chip">
          {chip}
          {chip.includes("could not") && (
            <button type="button" className="offline-link" data-testid="offline-feedback-retry" onClick={retry}>
              Retry
            </button>
          )}
        </span>
      )}
    </div>
    <StorageNotice storage={storage} />
    </div>
  );
};
