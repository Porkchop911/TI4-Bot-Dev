import React, { useCallback, useEffect, useState } from "react";
import { FeedbackSheet } from "./FeedbackSheet.tsx";
import {
  OPEN_FEEDBACK_EVENT,
  openFeedback,
  useFeedbackAvailable,
  useFeedbackDockActive,
} from "./FeedbackDockButton.tsx";
import "./feedback.css";

export { openFeedback };

/**
 * The floating Feedback button and its sheet, mounted once for the whole app. It only exists on
 * the public playtest server (`/health` says `public`), so local development stays unchanged.
 */
export const FeedbackLauncher: React.FC = () => {
  const available = useFeedbackAvailable();
  const inDock = useFeedbackDockActive();
  const [open, setOpen] = useState(false);
  const show = useCallback(() => setOpen(true), []);
  useEffect(() => {
    window.addEventListener(OPEN_FEEDBACK_EVENT, show);
    return () => window.removeEventListener(OPEN_FEEDBACK_EVENT, show);
  }, [show]);
  // An offline game has no server to ask: reports are queued and sent later, so the button is there.
  if (!available) return null;
  return (
    <>
      {/* On a phone in a game the dock carries the icon button instead of this floating pill. */}
      {!inDock && (
      <button
        type="button"
        className="feedback-fab"
        data-testid="feedback-button"
        aria-haspopup="dialog"
        onClick={show}
      >
        <svg width="16" height="16" viewBox="0 0 16 16" aria-hidden="true">
          <path
            d="M2 3h12v8H7l-3 3v-3H2z"
            fill="none"
            stroke="currentColor"
            strokeWidth="1.5"
            strokeLinejoin="round"
          />
        </svg>
        <span>Feedback</span>
      </button>
      )}
      <FeedbackSheet open={open} onClose={() => setOpen(false)} />
    </>
  );
};
