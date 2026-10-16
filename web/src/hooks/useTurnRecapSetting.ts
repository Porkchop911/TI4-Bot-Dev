import { useCallback, useSyncExternalStore } from "react";

export const TURN_RECAP_KEY = "player_turn_recap";

const listeners = new Set<() => void>();

export function readTurnRecapEnabled(): boolean {
  try {
    return localStorage.getItem(TURN_RECAP_KEY) === "true";
  } catch {
    return false;
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

/**
 * The viewer's choice to get one recap toast when another player's turn ends. Off by default,
 * stored in this browser only (like the toast mute), shared by every user of the hook.
 */
export function useTurnRecapSetting() {
  const enabled = useSyncExternalStore(subscribe, readTurnRecapEnabled, () => false);
  const setEnabled = useCallback((next: boolean) => {
    try {
      localStorage.setItem(TURN_RECAP_KEY, String(next));
    } catch {
      // Storage unavailable: the choice then only lasts until the next read.
    }
    listeners.forEach((l) => l());
  }, []);
  const toggle = useCallback((): boolean => {
    const next = !readTurnRecapEnabled();
    setEnabled(next);
    return next;
  }, [setEnabled]);
  return { enabled, setEnabled, toggle };
}
