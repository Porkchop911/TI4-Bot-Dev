import React, { useEffect, useLayoutEffect, useState } from "react";
import { AutoResolveToastContainer } from "./AutoResolveToast.tsx";
import { ToastHistory } from "./ToastHistory.tsx";
import { useNotificationLog } from "./NotificationLog.tsx";
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

// Controls near the top of the toasts' column the stack must stop below: the "Prepare your
// secondary" chip in the board's toolbar strip (run s2-36-0118: a tall stack covered it and the
// click on it timed out three times).
const TOP_OBSTACLES = [".secondary-prep__chip"];

/**
 * The tallest the column may be (from its bottom offset up) without covering a control near the
 * top of the window; null when nothing is in the way.
 */
export function measureMaxHeight(bottomOffset: number): number | null {
  let limit = 0;
  for (const selector of TOP_OBSTACLES)
    for (const el of document.querySelectorAll<HTMLElement>(selector)) {
      const rect = el.getBoundingClientRect();
      if (rect.height > 0 && rect.bottom < window.innerHeight * 0.6 && rect.left < 340)
        limit = Math.max(limit, rect.bottom + GAP);
    }
  return limit > 0 ? Math.max(0, window.innerHeight - bottomOffset - limit) : null;
}

/**
 * Mounted once in the game: other players' actions and auto-resolved decisions as corner toasts,
 * lifted above the turn action bar and the other bottom overlays.
 */
export const CornerToastLayer: React.FC<CornerToastsInput> = (props) => {
  const { notifications, dismiss, history } = useCornerToasts(props);
  const [offset, setOffset] = useState(GAP);
  const [maxHeight, setMaxHeight] = useState<number | null>(null);
  // A game keeps its notification log behind an icon beside the Objectives button (offline and
  // online alike); the pill under the toasts is only the fallback without a provider.
  const log = useNotificationLog();
  const { inline, setEntries } = log;
  useEffect(() => {
    if (inline) setEntries(history);
  }, [inline, history, setEntries]);
  const count = notifications.length;
  // The history toggle stays after the toasts have gone, so it counts as something on screen.
  const shown = count > 0 || history.length > 0;

  useLayoutEffect(() => {
    const update = () => {
      const bottom = measureBottomOffset();
      setOffset(bottom);
      setMaxHeight(measureMaxHeight(bottom));
    };
    update();
    window.addEventListener("resize", update);
    // Bars appear and disappear with decisions; keep tracking while something is on screen.
    const timer = shown ? window.setInterval(update, 500) : undefined;
    // ... and react in the next frame when the page changes (a bar growing is not a 500 ms matter).
    let frame = 0;
    const observer =
      shown && typeof MutationObserver !== "undefined"
        ? new MutationObserver(() => {
            if (frame) return;
            frame = window.requestAnimationFrame(() => {
              frame = 0;
              update();
            });
          })
        : undefined;
    observer?.observe(document.body, { childList: true, subtree: true, attributes: true, attributeFilter: ["class", "style", "hidden"] });
    return () => {
      window.removeEventListener("resize", update);
      if (timer !== undefined) window.clearInterval(timer);
      observer?.disconnect();
      if (frame) window.cancelAnimationFrame(frame);
    };
  }, [count, shown]);

  return (
    <AutoResolveToastContainer
      notifications={notifications}
      onDismiss={dismiss}
      bottomOffset={offset}
      maxHeight={maxHeight ?? undefined}
      underControls={inline}
      footer={inline ? undefined : <ToastHistory history={history} />}
    />
  );
};
