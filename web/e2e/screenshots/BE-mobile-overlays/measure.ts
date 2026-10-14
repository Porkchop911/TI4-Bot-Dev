import type { Page } from "@playwright/test";

/** What a pending overlay does to the map on a phone; measured in the page with getBoundingClientRect / elementsFromPoint. */
export interface OverlayMetrics {
  /** Share (0..1) of the map viewport whose topmost element is not the map (header / floating buttons included). */
  covered: number;
  /** Share of map sample points whose topmost element is the map itself (taps reach a planet / system). */
  reachable: number;
  /** Root element(s) of what covers the map, with their share of the sample points. */
  roots: string[];
  /** A visible control of the covering root that minimizes / collapses / hides it. */
  minimizeControl: string | null;
  /** Buttons / inputs of the covering root shorter or narrower than 44 px. */
  small: string[];
  /** Controls of the covering root that lie outside the viewport. */
  offscreen: string[];
  /** Sample points that belong to the map's own toolbar / legend (not covering; excluded from `covered` when passed back as `exclude`). */
  chrome: number[];
  /** Sample points whose topmost element is not the map (and not map chrome). */
  coveredIdx: number[];
  /** Sample points that counted (the grid minus the excluded ones); 0 when the page itself hides the whole map. */
  samples: number;
}

const GRID = 12;

/** Runs in the page. */
export async function measureMap(page: Page, exclude: number[] = []): Promise<OverlayMetrics> {
  return page.evaluate(([grid, skip]) => {
    const vp = document.querySelector<HTMLElement>('[data-testid="board-viewport"]') ?? document.body;
    const svg = document.querySelector('[data-testid="ti4-board-svg"]');
    const rect = vp.getBoundingClientRect();
    const x0 = Math.max(0, rect.left);
    const x1 = Math.min(window.innerWidth, rect.right);
    const y0 = Math.max(0, rect.top);
    const y1 = Math.min(window.innerHeight, rect.bottom);
    let covered = 0;
    let reachable = 0;
    let total = 0;
    const rootSet = new Map<Element, number>();
    const chrome: number[] = [];
    const coveredIdx: number[] = [];
    const labelOf = (el: Element) =>
      `${el.tagName.toLowerCase()}${el.getAttribute("data-testid") ? `[${el.getAttribute("data-testid")}]` : ""}${
        typeof (el as HTMLElement).className === "string" && (el as HTMLElement).className
          ? "." + (el as HTMLElement).className.trim().split(/\s+/).slice(0, 2).join(".")
          : ""
      }`;
    const rootOf = (el: Element): Element => {
      let cur: Element | null = el;
      let best: Element = el;
      while (cur && cur !== document.body) {
        const pos = getComputedStyle(cur).position;
        if (pos === "fixed" || pos === "absolute" || pos === "sticky") best = cur;
        cur = cur.parentElement;
      }
      return best;
    };
    if (x1 > x0 && y1 > y0)
      for (let i = 0; i < grid; i++)
        for (let j = 0; j < grid; j++) {
          const x = x0 + ((i + 0.5) / grid) * (x1 - x0);
          const y = y0 + ((j + 0.5) / grid) * (y1 - y0);
          const index = i * grid + j;
          const top = document.elementFromPoint(x, y);
          if (skip.includes(index)) continue;
          total++;
          if (!top) continue;
          if (top.closest(".board-chrome") && vp.contains(top)) chrome.push(index);
          if ((svg && svg.contains(top)) || (vp.contains(top) && !top.closest(".board-chrome"))) {
            reachable++;
          } else {
            covered++;
            coveredIdx.push(index);
            const r = rootOf(top);
            rootSet.set(r, (rootSet.get(r) ?? 0) + 1);
          }
        }
    const roots = [...rootSet.entries()].sort((a, b) => b[1] - a[1]).slice(0, 3);
    const visible = (el: Element) => {
      const r = el.getBoundingClientRect();
      const s = getComputedStyle(el);
      return r.width > 0 && r.height > 0 && s.visibility !== "hidden" && s.display !== "none";
    };
    const small: string[] = [];
    const offscreen: string[] = [];
    let minimizeControl: string | null = null;
    for (const [root] of roots) {
      for (const el of root.querySelectorAll<HTMLElement>("button, [role=button], input, select, a[href], summary")) {
        if (!visible(el)) continue;
        const r = el.getBoundingClientRect();
        const name = (el.getAttribute("aria-label") ?? el.textContent ?? el.getAttribute("data-testid") ?? el.tagName).trim().slice(0, 28);
        if (Math.min(r.width, r.height) < 43.5 && (el as HTMLInputElement).type !== "hidden")
          small.push(`${name} ${Math.round(r.width)}x${Math.round(r.height)}`);
        if (r.right < 0 || r.left > window.innerWidth || r.bottom < 0 || r.top > window.innerHeight)
          offscreen.push(`${name} @${Math.round(r.left)},${Math.round(r.top)}`);
        const id = `${el.getAttribute("data-testid") ?? ""} ${el.getAttribute("aria-label") ?? ""} ${el.textContent ?? ""}`;
        if (!minimizeControl && /minimi[sz]e|collapse|hide|show map|view map|peek|to the map|dismiss|close|fold/i.test(id))
          minimizeControl = el.getAttribute("data-testid") || name;
      }
    }
    return {
      covered: total ? covered / total : 0,
      reachable: total ? reachable / total : 0,
      roots: roots.map(([r, n]) => `${labelOf(r)}:${Math.round((n / total) * 100)}%`),
      minimizeControl,
      small: small.slice(0, 12),
      offscreen: offscreen.slice(0, 8),
      chrome,
      coveredIdx,
      samples: total,
    };
  }, [GRID, exclude] as [number, number[]]);
}
