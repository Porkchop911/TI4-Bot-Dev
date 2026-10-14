import { useCallback, useSyncExternalStore } from "react";

export const AUTO_SUBMIT_KEY = "player_auto_submit_lone";

const listeners = new Set<() => void>();

/** Default ON: only an explicit "false" turns the lone-case auto-submit off. */
export function readAutoSubmitLone(): boolean {
  try {
    return localStorage.getItem(AUTO_SUBMIT_KEY) !== "false";
  } catch {
    return true;
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

/** Viewer-local switch for answering a lone strategic action / lone activation automatically. */
export function useAutoSubmitSetting() {
  const enabled = useSyncExternalStore(subscribe, readAutoSubmitLone, () => true);
  const setEnabled = useCallback((next: boolean) => {
    try {
      localStorage.setItem(AUTO_SUBMIT_KEY, String(next));
    } catch {
      // Storage unavailable: the choice then only lasts until the next read.
    }
    listeners.forEach((l) => l());
  }, []);
  const toggle = useCallback(() => setEnabled(!readAutoSubmitLone()), [setEnabled]);
  return { enabled, setEnabled, toggle };
}
