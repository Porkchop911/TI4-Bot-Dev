import React, { useLayoutEffect, useState } from "react";
import { AutoResolveToastContainer } from "./AutoResolveToast.tsx";
import { ToastHistory } from "./ToastHistory.tsx";
import { CornerToastsInput, useCornerToasts } from "../hooks/useCornerToasts.ts";

// Fixed bars at the bottom edge the toasts must clear: the turn action bar, the event log toggle
// and a decision banner.
const BOTTOM_OBSTACLES = [".turn-bar", ".app-shell__event-log", ".choice-banner"];
const GAP = 12;

/** The distance from the window's bottom edge that clears every visible bottom bar. */
export function measureBottomOffset(): number {
  let top = window.innerHeight;
  for (const selector of BOTTOM_OBSTACLES)
    for (const el of document.querySelectorAll<HTMLElement>(selector)) {
      const rect = el.getBoundingClientRect();
      // Only bars that sit in the lower part of the window and overlap the toasts' column.
      if (rect.height > 0 && rect.top > window.innerHeight * 0.4 && rect.left < 340)
        top = Math.min(top, rect.top);
    }
  return window.innerHeight - top + GAP;
}

/**
 * Mounted once in the game: other players' actions and auto-resolved decisions as corner toasts,
 * lifted above the turn action bar and the other bottom overlays.
 */
export const CornerToastLayer: React.FC<CornerToastsInput> = (props) => {
  const { notifications, dismiss, history } = useCornerToasts(props);
  const [offset, setOffset] = useState(GAP);
  const count = notifications.length;

  useLayoutEffect(() => {
    const update = () => setOffset(measureBottomOffset());
    update();
    window.addEventListener("resize", update);
    // Bars appear and disappear with decisions; keep tracking while something is on screen.
    const timer = count > 0 ? window.setInterval(update, 500) : undefined;
    return () => {
      window.removeEventListener("resize", update);
      if (timer !== undefined) window.clearInterval(timer);
    };
  }, [count]);

  return (
    <AutoResolveToastContainer
      notifications={notifications}
      onDismiss={dismiss}
      bottomOffset={offset}
      footer={<ToastHistory history={history} />}
    />
  );
};
