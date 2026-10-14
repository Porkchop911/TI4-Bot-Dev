import { useCallback, useSyncExternalStore } from "react";

export type SecondaryPrepMode = "review" | "auto";
export const SECONDARY_PREP_MODE_KEY = "player_secondary_prep_mode";

const listeners = new Set<() => void>();

/**
 * "Review before play" is the default for everybody; only an explicit stored "auto" turns on
 * auto-play. Unreadable or unexpected storage means review.
 */
export function readSecondaryPrepMode(): SecondaryPrepMode {
  try {
    return localStorage.getItem(SECONDARY_PREP_MODE_KEY) === "auto" ? "auto" : "review";
  } catch {
    return "review";
  }
}

function subscribe(listener: () => void) {
  listeners.add(listener);
  window.addEventListener("storage", listener);
  return () => {
    listeners.delete(listener);
    window.removeEventListener("storage", listener);
  };
}

/** Viewer-local choice of what happens when a prepared secondary's window opens. */
export function useSecondaryPrepMode() {
  const mode = useSyncExternalStore(subscribe, readSecondaryPrepMode, () => "review" as const);
  const setMode = useCallback((next: SecondaryPrepMode) => {
    try {
      localStorage.setItem(SECONDARY_PREP_MODE_KEY, next);
    } catch {
      // Storage unavailable: the choice then only lasts until the next read.
    }
    listeners.forEach((l) => l());
  }, []);
  return { mode, setMode };
}
