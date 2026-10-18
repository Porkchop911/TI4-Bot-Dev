import { useCallback, useSyncExternalStore } from "react";

export const ASK_I48S_KEY = "player_ask_i48s";

const listeners = new Set<() => void>();

/** Default ON (ask every time): only an explicit "false" stops the I48S prompt from being shown. */
export function readAskI48s(): boolean {
  try {
    return localStorage.getItem(ASK_I48S_KEY) !== "false";
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

/** Viewer-local switch: ask about the L1Z1X agent I48S after each activation, or decline for me. */
export function useI48sSetting() {
  const ask = useSyncExternalStore(subscribe, readAskI48s, () => true);
  const setAsk = useCallback((next: boolean) => {
    try {
      localStorage.setItem(ASK_I48S_KEY, String(next));
    } catch {
      // Storage unavailable: the choice then only lasts until the next read.
    }
    listeners.forEach((l) => l());
  }, []);
  const toggle = useCallback(() => setAsk(!readAskI48s()), [setAsk]);
  return { ask, setAsk, toggle };
}
