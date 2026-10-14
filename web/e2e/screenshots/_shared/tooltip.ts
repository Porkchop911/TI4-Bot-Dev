import type { Locator, Page } from "@playwright/test";

/**
 * Chrome draws native `title` tooltips outside the page, so a screenshot never contains them.
 * This draws the same text where the browser would, and rings the hovered element, so the
 * picture shows exactly what a player reads. Custom CSS tooltips (hover/focus) need no help:
 * use `locator.hover()` / `locator.focus()` for those.
 */
export async function showTitleTooltip(page: Page, target: Locator, text?: string) {
  await target.scrollIntoViewIfNeeded();
  await target.hover();
  await target.evaluate(
    (element, override) => {
      const title = override ?? element.getAttribute("title") ?? element.closest("[title]")?.getAttribute("title");
      if (!title) throw new Error("showTitleTooltip: element has no title text");
      const box = element.getBoundingClientRect();
      const ring = document.createElement("div");
      Object.assign(ring.style, {
        position: "fixed",
        left: `${box.left - 3}px`,
        top: `${box.top - 3}px`,
        width: `${box.width + 6}px`,
        height: `${box.height + 6}px`,
        border: "2px solid #38bdf8",
        borderRadius: "6px",
        pointerEvents: "none",
        zIndex: "2147483646",
      });
      ring.dataset.screenshotOverlay = "ring";
      const tip = document.createElement("div");
      tip.textContent = title;
      Object.assign(tip.style, {
        position: "fixed",
        maxWidth: "340px",
        padding: "6px 9px",
        font: "13px/1.35 system-ui, sans-serif",
        whiteSpace: "pre-wrap",
        color: "#1b1b1b",
        background: "#f4f4f4",
        border: "1px solid #9a9a9a",
        boxShadow: "0 2px 8px rgba(0,0,0,.45)",
        pointerEvents: "none",
        zIndex: "2147483647",
      });
      tip.dataset.screenshotOverlay = "tooltip";
      document.body.append(ring, tip);
      const tipBox = tip.getBoundingClientRect();
      const below = box.bottom + 8;
      const top = below + tipBox.height < window.innerHeight ? below : Math.max(4, box.top - 8 - tipBox.height);
      const left = Math.min(Math.max(4, box.left), window.innerWidth - tipBox.width - 4);
      tip.style.top = `${top}px`;
      tip.style.left = `${left}px`;
    },
    text,
  );
}
