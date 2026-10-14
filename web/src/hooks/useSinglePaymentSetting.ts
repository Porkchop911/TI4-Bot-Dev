import { useCallback, useSyncExternalStore } from "react";

export const SINGLE_PAYMENT_KEY = "player_single_production_payment";

const listeners = new Set<() => void>();

/** Default ON: only an explicit "false" brings back one payment question per build. */
export function readSinglePayment(): boolean {
  try {
    return localStorage.getItem(SINGLE_PAYMENT_KEY) !== "false";
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

/** Viewer-local switch: pay once for a whole production instead of asking for each build. */
export function useSinglePaymentSetting() {
  const enabled = useSyncExternalStore(subscribe, readSinglePayment, () => true);
  const setEnabled = useCallback((next: boolean) => {
    try {
      localStorage.setItem(SINGLE_PAYMENT_KEY, String(next));
    } catch {
      // Storage unavailable: the choice then only lasts until the next read.
    }
    listeners.forEach((l) => l());
  }, []);
  return { enabled, setEnabled };
}
