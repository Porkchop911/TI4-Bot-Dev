import { expect, type Locator, type Page, type TestInfo } from "@playwright/test";
import { shot } from "../_shared/shot";

/** "before" is captured on the base commit (unified-2026-10-08); the default regenerates the current code. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const VIEWPORT = { width: 1280, height: 720 };

const headerGeometry = (page: Page) =>
  page.evaluate(() => {
    const header = document.querySelector(".app-shell__header")!;
    const overlays = document.querySelector(".app-shell__overlays")!;
    const z = (el: Element) => Number(getComputedStyle(el).zIndex) || 0;
    return { headerBottom: header.getBoundingClientRect().bottom, overlaysZ: z(overlays), headerZ: z(header) };
  });

/**
 * Shots the viewport and, for the current code, asserts the panel is not under the app header:
 * either its top is at or below the header's bottom, or the overlays layer is stacked above the header.
 */
export async function shotAndCheck(page: Page, testInfo: TestInfo, name: string, panel: Locator) {
  await panel.waitFor();
  await shot(page, testInfo, `${prefix}-${name}`);
  const geometry = await headerGeometry(page);
  const top = (await panel.boundingBox())!.y;
  console.log(`${name}: panel top ${top}, header bottom ${geometry.headerBottom}, overlays z ${geometry.overlaysZ}, header z ${geometry.headerZ}`);
  if (prefix === "before") return;
  expect(top >= geometry.headerBottom || geometry.overlaysZ > geometry.headerZ, `${name}: panel hidden under the header`).toBe(true);
}
