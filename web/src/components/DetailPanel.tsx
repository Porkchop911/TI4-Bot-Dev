import React, { useEffect, useLayoutEffect, useRef, useState } from "react";
import { Drawer } from "../primitives/index.ts";

/**
 * Publishes how far the tallest confirm bar (activation, planet selection, payment) reaches up from the
 * bottom edge as --bar-clear, so the panel ends above it: the bar grows with an error line or a wrapped
 * label and used to lie under the panel (Confirm Activation and Cancel were unreachable).
 */
function useConfirmBarClearance() {
  useLayoutEffect(() => {
    const root = document.documentElement;
    const resizer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(() => measure());
    let observed: Element[] = [];
    function measure() {
      const bars = Array.from(document.querySelectorAll(".system-activation-bar"));
      if (bars.length !== observed.length || bars.some((bar, i) => bar !== observed[i])) {
        resizer?.disconnect();
        bars.forEach((bar) => resizer?.observe(bar));
        observed = bars;
      }
      const clear = bars.reduce((max, bar) => {
        const rect = bar.getBoundingClientRect();
        return rect.height > 0 ? Math.max(max, Math.ceil(window.innerHeight - rect.top) + 12) : max;
      }, 0);
      root.style.setProperty("--bar-clear", `${clear}px`);
    }
    measure();
    const mutations = new MutationObserver(measure);
    mutations.observe(document.body, { childList: true, subtree: true });
    window.addEventListener("resize", measure);
    return () => {
      mutations.disconnect();
      resizer?.disconnect();
      window.removeEventListener("resize", measure);
      root.style.removeProperty("--bar-clear");
    };
  }, []);
}

/** Shared, persistent inspection surface for map and card details. */
export const DetailPanel: React.FC<{
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  testId?: string;
  closeTestId?: string;
}> = ({
  title,
  onClose,
  children,
  testId = "detail-panel",
  closeTestId = "close-detail-button",
}) => {
  const closeRef = useRef<HTMLButtonElement>(null);
  useConfirmBarClearance();
  // Phone only (the fold control and styles exist in the compact layout): the panel covers most of the
  // map, so it folds to a slim bar with the title that keeps the map tappable. Stays folded while the
  // title changes (tapping another system), and is gone with the panel.
  const [folded, setFolded] = useState(false);
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeRef.current?.focus();
    return () => {
      if (document.activeElement === closeRef.current) previous?.focus();
    };
  }, []);
  return (
    <Drawer
      open
      onClose={onClose}
      modal={false}
      ariaLabel={`${title} details`}
      data-testid={testId}
      className={`detail-panel panel${folded ? " detail-panel--folded" : ""}`}
    >
      <div className="detail-panel__header">
        <h2>{title}</h2>
        <button
          type="button"
          className="button button--secondary button--icon detail-panel__fold"
          data-testid={`${testId}-fold`}
          aria-expanded={!folded}
          aria-label={folded ? `Show ${title} details` : `Fold ${title} details`}
          onClick={() => setFolded((value) => !value)}
        >
          {folded ? "▴" : "▾"}
        </button>
        <button
          ref={closeRef}
          type="button"
          className="button button--secondary button--icon"
          data-testid={closeTestId}
          aria-label={`Close ${title} details`}
          onClick={onClose}
        >
          ✕
        </button>
      </div>
      <div className="detail-panel__body">{children}</div>
    </Drawer>
  );
};
