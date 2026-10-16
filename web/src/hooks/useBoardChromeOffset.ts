import { useLayoutEffect, type RefObject } from "react";

/** The CSS variable holding the bottom edge of the board's toolbar row, for overlays that must sit below it. */
export const BOARD_CHROME_BOTTOM_VAR = "--board-chrome-bottom";

/** The positioned element that holds the board and its overlays; the offset is measured from its top. */
export const BOARD_ANCHOR_SELECTOR = ".app-shell__board";

/**
 * The element's bottom edge relative to the top of the box that contains the overlay. Both rects are
 * viewport coordinates, so scrolling and any header above the board cancel out.
 */
export function chromeBottomWithin(chromeBottom: number, containerTop: number): number {
  return Math.max(0, Math.ceil(chromeBottom - containerTop));
}

/**
 * Publishes the element's bottom edge as `--board-chrome-bottom` on the document root, in px measured
 * from the top of the board column (`.app-shell__board`, where the floating secondary-prep panels are
 * mounted; the element's offset parent when there is none), while it is mounted, and clears it on
 * unmount. The row wraps (toolbar, seat legend, prepare chip), so its height varies.
 */
export function useBoardChromeOffset(ref: RefObject<HTMLElement | null>): void {
  useLayoutEffect(() => {
    const el = ref.current;
    if (!el) return;
    const root = document.documentElement;
    const container = (): Element | null => el.closest(BOARD_ANCHOR_SELECTOR) ?? el.offsetParent;
    const publish = () => {
      const top = container()?.getBoundingClientRect().top ?? 0;
      root.style.setProperty(
        BOARD_CHROME_BOTTOM_VAR,
        `${chromeBottomWithin(el.getBoundingClientRect().bottom, top)}px`,
      );
    };
    publish();
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(publish);
    observer?.observe(el);
    const parent = container();
    if (parent) observer?.observe(parent);
    window.addEventListener("resize", publish);
    return () => {
      observer?.disconnect();
      window.removeEventListener("resize", publish);
      root.style.removeProperty(BOARD_CHROME_BOTTOM_VAR);
    };
  }, [ref]);
}
