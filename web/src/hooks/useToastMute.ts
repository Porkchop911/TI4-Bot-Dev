import { useCallback, useSyncExternalStore } from "react";

export const TOAST_MUTE_KEY = "player_toasts_muted";

const listeners = new Set<() => void>();

export function readToastsMuted(): boolean {
  try {
    return localStorage.getItem(TOAST_MUTE_KEY) === "true";
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
 * The user's choice to silence other players' action notifications, persisted in localStorage like
 * the turn sound. Every user of the hook (the toggle, the toast layer) stays in sync.
 */
export function useToastMute() {
  const muted = useSyncExternalStore(subscribe, readToastsMuted, () => false);
  const setMuted = useCallback((next: boolean) => {
    try {
      localStorage.setItem(TOAST_MUTE_KEY, String(next));
    } catch {
      // Storage unavailable: the choice then only lasts until the next read.
    }
    listeners.forEach((l) => l());
  }, []);
  const toggleMute = useCallback((): boolean => {
    const next = !readToastsMuted();
    setMuted(next);
    return next;
  }, [setMuted]);
  return { muted, setMuted, toggleMute };
}
