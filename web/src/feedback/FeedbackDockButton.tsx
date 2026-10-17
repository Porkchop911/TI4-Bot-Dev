import React, { useEffect, useSyncExternalStore } from "react";
import { useBuildInfo } from "../buildInfo.ts";
import { useMatchMedia } from "../hooks/useCompactLayout.ts";
import { useOfflineGameActive } from "../offline/useOfflineLauncher.ts";

export const OPEN_FEEDBACK_EVENT = "ti4:open-feedback";

/** Opens the feedback sheet from anywhere (the phone menu, the dock, the lobby). */
export function openFeedback(): void {
  window.dispatchEvent(new Event(OPEN_FEEDBACK_EVENT));
}

/** The phone dock (Players / Objectives / bell / Events) is shown up to this width, see index.css. */
export const DOCK_QUERY = "(max-width: 720px)";

let dockCount = 0;
const listeners = new Set<() => void>();
const emit = () => listeners.forEach((l) => l());
const subscribe = (l: () => void) => {
  listeners.add(l);
  return () => {
    listeners.delete(l);
  };
};

/** True while a game's phone dock carries the Feedback button (the floating pill then steps aside). */
export function useFeedbackDockActive(): boolean {
  return useSyncExternalStore(
    subscribe,
    () => dockCount > 0,
    () => false,
  );
}

/** Whether the Feedback button exists at all: the public server, or an offline game (queued reports). */
export function useFeedbackAvailable(): boolean {
  const info = useBuildInfo();
  const offlineGame = useOfflineGameActive();
  return info.isPublic || offlineGame;
}

/**
 * Whether the game shell's phone dock should show the Feedback icon. Registers the dock as the
 * button's home so the floating pill hides; wider than a phone the pill stays where it was.
 */
export function useFeedbackDock(): boolean {
  const available = useFeedbackAvailable();
  const narrow = useMatchMedia(DOCK_QUERY);
  const inDock = available && narrow;
  useEffect(() => {
    if (!inDock) return;
    dockCount += 1;
    emit();
    return () => {
      dockCount -= 1;
      emit();
    };
  }, [inDock]);
  return inDock;
}

/** Icon-only Feedback button for the phone dock; opens the same sheet as the floating pill. */
export const FeedbackDockButton: React.FC = () => (
  <button
    type="button"
    className="button button--secondary app-shell__dock-icon feedback-dock-button"
    data-testid="feedback-button"
    aria-label="Feedback"
    title="Feedback"
    aria-haspopup="dialog"
    onClick={openFeedback}
  >
    <svg width="20" height="20" viewBox="0 0 16 16" aria-hidden="true">
      <path
        d="M2 3h12v8H7l-3 3v-3H2z"
        fill="none"
        stroke="currentColor"
        strokeWidth="1.5"
        strokeLinejoin="round"
      />
    </svg>
  </button>
);
