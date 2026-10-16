import React, { useEffect, useState } from "react";

/** True once `active` has stayed true for `delayMs`; a blip shorter than that never shows. */
export function useAfterDelay(active: boolean, delayMs: number): boolean {
  const [shown, setShown] = useState(false);
  useEffect(() => {
    if (!active) {
      setShown(false);
      return;
    }
    const timer = window.setTimeout(() => setShown(true), delayMs);
    return () => window.clearTimeout(timer);
  }, [active, delayMs]);
  return active && shown;
}

/** Small, non-blocking chip while the page is reconnecting by itself. Ignores pointer input. */
export const ReconnectingChip: React.FC<{ visible: boolean }> = ({ visible }) =>
  visible ? (
    <div className="connection-indicator" role="status" data-testid="lobby-connection-lost">
      Reconnecting…
    </div>
  ) : null;

/**
 * Shown when the page gave up reconnecting by itself or the game is gone: says what is wrong in
 * plain words and offers the way out. Never a raw network error string.
 */
export const ConnectionProblem: React.FC<{
  kind: "gone" | "unreachable";
  message: string;
  onRetry: () => void;
}> = ({ kind, message, onRetry }) => (
  <div className="connection-problem" role="alert" data-testid="resume-fatal" data-kind={kind}>
    <div className="connection-problem__card">
      <h2 className="connection-problem__title">
        {kind === "gone" ? "Game not found" : "Connection lost"}
      </h2>
      <p>{message}</p>
      <div className="connection-problem__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="resume-retry"
          onClick={onRetry}
        >
          Retry
        </button>
        <a className="button button--secondary" href="/" data-testid="resume-back">
          Back to start page
        </a>
      </div>
    </div>
  </div>
);
