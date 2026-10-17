import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { DOCK_IDS, dockRects, openScene, PHONES, SIZES, tappable } from "./scene";

// The bell is in the dock of an online game: five buttons, one row, 44 px targets, unread badge.
for (const size of PHONES)
  test(`dock with the bell and unread badge ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size);
    const bell = page.getByTestId("toast-history-mobile-toggle");
    await expect(bell).toBeVisible();
    await expect(bell).toHaveAttribute("aria-label", /Notifications, \d+ new/);
    await expect(page.getByText(/Recent notifications \(/)).toHaveCount(0);
    const rects = await dockRects(page);
    expect(rects).toHaveLength(5);
    for (let i = 0; i < rects.length; i++) {
      expect(rects[i].h, `button ${i} height`).toBeGreaterThanOrEqual(43.5);
      expect(rects[i].w, `button ${i} width`).toBeGreaterThanOrEqual(43.5);
      expect(Math.abs(rects[i].t - rects[0].t)).toBeLessThan(2);
      if (i) expect(rects[i].l).toBeGreaterThanOrEqual(rects[i - 1].r + 3);
    }
    expect(rects[0].l).toBeGreaterThanOrEqual(0);
    expect(rects[4].r).toBeLessThanOrEqual(SIZES[size].width);
    for (const id of DOCK_IDS) expect(await tappable(page, id), id).toBe(true);
    await shot(page, testInfo, `dock-${size}`);
  });

test("desktop: the bell sits in the header row next to Objectives", async ({ page }, testInfo) => {
  await openScene(page, "desktop");
  const bell = page.getByTestId("toast-history-toggle");
  await expect(bell).toBeVisible();
  const o = (await page.getByTestId("objectives-modal-button").boundingBox())!;
  const b = (await bell.boundingBox())!;
  expect(Math.abs(b.y - o.y)).toBeLessThan(6);
  await expect(page.getByText(/Recent notifications \(/)).toHaveCount(0);
  await shot(page, testInfo, "header-desktop");
});
