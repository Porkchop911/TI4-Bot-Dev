import { useLayoutEffect, type RefObject } from "react";

/** The CSS variable holding the height of the persistent bottom bar, for overlays that must sit above it. */
export const BOTTOM_BAR_OFFSET_VAR = "--bottom-bar-offset";

/**
 * Publishes the element's height as `--bottom-bar-offset` on the document root while it is
 * mounted, and clears it on unmount. The event log docks above the turn action bar with it.
 */
export function useBottomBarOffset(ref: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const root = document.documentElement;
    const publish = () =>
      root.style.setProperty(BOTTOM_BAR_OFFSET_VAR, `${Math.ceil(el.getBoundingClientRect().height)}px`);
    publish();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(publish);
    observer?.observe(el);
    window.addEventListener("resize", publish);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", publish);
      root.style.removeProperty(BOTTOM_BAR_OFFSET_VAR);
    };
  }, [ref]);
}
