import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene, PHONES, PORTRAIT, prefix, SIZES, tappable, type SizeName } from "./scene";

// The Feedback pill floated over the map above the dock; on a portrait phone it is now an icon in the dock.
for (const size of [...PHONES, "desktop"] as SizeName[])
  test(`${prefix} dock with the feedback button ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, "turn");
    const button = page.getByTestId("feedback-button");
    await expect(button).toBeVisible();
    const box = (await button.boundingBox())!;
    console.log(`MEASURE ${size}: feedback ${Math.round(box.width)}x${Math.round(box.height)} at ${Math.round(box.x)},${Math.round(box.y)}`);
    if (prefix === "after") {
      expect(box.height).toBeGreaterThanOrEqual(43.5);
      const inDock = (await page.locator(".app-shell__mobile-actions").getByTestId("feedback-button").count()) === 1;
      expect(inDock).toBe(PORTRAIT.includes(size));
      if (inDock) {
        await expect(button).toHaveAttribute("aria-label", "Feedback");
        await expect(button).toHaveAttribute("title", "Feedback");
        expect(box.width).toBeGreaterThanOrEqual(43.5);
        expect(box.width).toBeLessThanOrEqual(48);
        expect(box.x).toBeGreaterThanOrEqual(0);
        expect(box.x + box.width).toBeLessThanOrEqual(SIZES[size].width);
        // One row with the other dock buttons, none overlapping.
        const rects = await page
          .locator(".app-shell__mobile-actions > *")
          .evaluateAll((els) => els.map((e) => e.getBoundingClientRect()).map((r) => ({ l: r.left, r: r.right, t: r.top })));
        for (let i = 1; i < rects.length; i++) {
          expect(rects[i].l).toBeGreaterThanOrEqual(rects[i - 1].r - 0.5);
          expect(Math.abs(rects[i].t - rects[0].t)).toBeLessThan(12);
        }
        expect(await tappable(page, "feedback-button")).toBe(true);
        expect(await page.locator(".feedback-fab").count()).toBe(0);
      }
    }
    await shot(page, testInfo, `${prefix}-dock-${size}`);
    // It still opens the feedback flow.
    await button.evaluate((el) => (el as HTMLElement).click());
    await expect(page.getByTestId("feedback-sheet")).toBeVisible();
  });
