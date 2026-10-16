import React, { useCallback, useEffect, useState } from "react";
import { useBuildInfo } from "../buildInfo.ts";
import { FeedbackSheet } from "./FeedbackSheet.tsx";
import { useOfflineGameActive } from "../offline/useOfflineLauncher.ts";
import "./feedback.css";

const OPEN_EVENT = "ti4:open-feedback";

/** Opens the feedback sheet from anywhere (the phone menu, the lobby). */
export function openFeedback(): void {
  window.dispatchEvent(new Event(OPEN_EVENT));
}

/**
 * The floating Feedback button and its sheet, mounted once for the whole app. It only exists on
 * the public playtest server (`/health` says `public`), so local development stays unchanged.
 */
export const FeedbackLauncher: React.FC = () => {
  const info = useBuildInfo();
  const [open, setOpen] = useState(false);
  const offlineGame = useOfflineGameActive();
  const show = useCallback(() => setOpen(true), []);
  useEffect(() => {
    window.addEventListener(OPEN_EVENT, show);
    return () => window.removeEventListener(OPEN_EVENT, show);
  }, [show]);
  // An offline game has no server to ask: reports are queued and sent later, so the button is there.
  if (!info.isPublic && !offlineGame) return null;
  return (
    <>
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
      <FeedbackSheet open={open} onClose={() => setOpen(false)} />
    </>
  );
};
