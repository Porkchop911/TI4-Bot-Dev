import { expect, type Browser, type Page } from "@playwright/test";
import { DEVICES, type DeviceName } from "../BN-phone-detection/scene";
import { openProduction, stageAndConfirm, confirmBuilds, waitForPanel } from "../AZ-production-single-payment/scene";

export { openProduction, stageAndConfirm, confirmBuilds, waitForPanel };

/** `before` shots come from the commit before the change (assertions on the new UI are skipped). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export type Size = { label: string; device: DeviceName | "desktop"; size?: { width: number; height: number } };

/** The phone sizes the change is for, plus the desktop the layout must not change on. */
export const SIZES: Size[] = [
  { label: "390x844", device: "mobile" },
  { label: "360x740", device: "mobile", size: { width: 360, height: 740 } },
  { label: "844x390", device: "mobile-landscape" },
  { label: "desktop-site", device: "desktop-site" },
  { label: "desktop", device: "desktop" },
];

export async function newPage(browser: Browser, size: Size, baseURL?: string) {
  const options = size.device === "desktop" ? { viewport: { width: 1440, height: 900 } } : DEVICES[size.device];
  const context = await browser.newContext({ ...options, colorScheme: "dark", baseURL });
  const page = await context.newPage();
  if (size.size) await page.setViewportSize(size.size);
  return { context, page };
}

export const isPhone = (size: Size) => size.device !== "desktop";

/** True when the element is fully inside the viewport. */
export async function inViewport(page: Page, testId: string): Promise<boolean> {
  return page.getByTestId(testId).evaluate((el) => {
    const r = el.getBoundingClientRect();
    return r.top >= 0 && r.left >= 0 && r.bottom <= window.innerHeight + 0.5 && r.right <= window.innerWidth + 0.5;
  });
}

/** The same visible-without-scrolling check for the confirm button found by its accessible name. */
export async function buttonInViewport(page: Page, name: string): Promise<boolean> {
  const button = name === "Pay" ? page.getByTestId("confirm-payment-btn") : page.getByRole("button", { name });
  return button.evaluate((el) => {
    const r = el.getBoundingClientRect();
    return r.top >= 0 && r.bottom <= window.innerHeight + 0.5;
  });
}

export interface Columns {
  count: number;
  distinctLefts: number;
  minWidth: number;
  heights: number[];
}

/** Lays out the rects of the elements matching a selector: how many distinct left edges they have. */
export async function columnsOf(page: Page, selector: string): Promise<Columns> {
  return page.evaluate((sel) => {
    const rects = Array.from(document.querySelectorAll(sel)).map((el) => el.getBoundingClientRect());
    const lefts = new Set(rects.map((r) => Math.round(r.left)));
    return {
      count: rects.length,
      distinctLefts: lefts.size,
      minWidth: Math.min(...rects.map((r) => r.width)),
      heights: rects.map((r) => Math.round(r.height)),
    };
  }, selector);
}

export async function assertOneColumn(page: Page, selector: string, minCount = 2) {
  const c = await columnsOf(page, selector);
  expect(c.count, `${selector} rows`).toBeGreaterThanOrEqual(minCount);
  expect(c.distinctLefts, `${selector} share one left edge`).toBe(1);
}
