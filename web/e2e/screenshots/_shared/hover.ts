import type { Locator, Page, TestInfo } from "@playwright/test";
import { freeze, shot } from "./shot";
import { showTitleTooltip } from "./tooltip";

export interface HoverShotOptions {
  /** Margin around the element and its tooltip (default 48). */
  pad?: number;
  /** Locator of a custom tooltip that must be visible before the shot. */
  tooltip?: Locator;
  /** Draw the element's native title text (browsers do not render it into screenshots). */
  native?: boolean;
  /** Text to draw instead of the element's own title (native only). */
  text?: string;
}

/**
 * Hovers (or focuses) `target`, waits for its tooltip, and writes a cropped screenshot that
 * includes the element and the tooltip. Native `title` tooltips are drawn by showTitleTooltip.
 */
export async function hoverShot(
  page: Page,
  testInfo: TestInfo,
  name: string,
  target: Locator,
  options: HoverShotOptions = {},
) {
  await target.scrollIntoViewIfNeeded();
  if (options.native) await showTitleTooltip(page, target, options.text);
  else {
    await target.hover();
    // The tooltip primitive opens after a short delay.
    if (options.tooltip) await options.tooltip.waitFor();
    else await page.locator('[role="tooltip"]').first().waitFor({ timeout: 2500 }).catch(() => page.waitForTimeout(300));
  }
  await page.waitForTimeout(400); // let the tooltip finish its entrance
  await freeze(page);
  // Crop around the element and every tooltip-like box that is showing, so the picture holds both.
  const boxes = [await target.boundingBox()];
  const tips = [
    ...(options.tooltip ? [options.tooltip] : []),
    page.locator('[role="tooltip"]'),
    page.locator(".vp-breakdown-tooltip"),
    page.locator(".combat-unit-row__roll-tooltip"),
    page.locator("[data-screenshot-overlay]"),
  ];
  for (const tip of tips) for (const el of await tip.all()) if (await el.isVisible()) boxes.push(await el.boundingBox());
  const real = boxes.filter((b): b is NonNullable<typeof b> => !!b);
  const view = page.viewportSize() ?? { width: 1440, height: 900 };
  const margin = options.pad ?? 48;
  const left = Math.max(0, Math.min(...real.map((b) => b.x)) - margin);
  const top = Math.max(0, Math.min(...real.map((b) => b.y)) - margin);
  const right = Math.min(view.width, Math.max(...real.map((b) => b.x + b.width)) + margin);
  const bottom = Math.min(view.height, Math.max(...real.map((b) => b.y + b.height)) + margin);
  await page.screenshot({
    path: `${testInfo.file.replace(/[^/]+$/, "")}out/${name}.png`,
    animations: "disabled",
    caret: "hide",
    clip: { x: left, y: top, width: right - left, height: bottom - top },
  });
}
export { shot };
