import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { innerChoice, openStep, outerChoice, prefix } from "./scene";

for (const [name, size] of [["desktop", null], ["phone", { width: 390, height: 844 }]] as const) {
  test(`window with several cards, ${name}`, async ({ page }, testInfo) => {
    if (size) await page.setViewportSize(size);
    await openStep(page, outerChoice);
    await expect(page.getByTestId("pass-reaction-btn")).toBeVisible();
    await shot(page, testInfo, `${prefix}-1-window-${name}`);
  });

  test(`card step, ${name}`, async ({ page }, testInfo) => {
    if (size) await page.setViewportSize(size);
    await openStep(page, innerChoice);
    await expect(page.getByTestId("play-reaction-btn-sh1")).toBeVisible();
    if (prefix === "after") {
      const back = page.getByTestId("pass-reaction-btn");
      await expect(back).toBeVisible();
      await expect(back).toContainText("Back");
      const box = await back.boundingBox();
      expect(box!.y + box!.height).toBeLessThanOrEqual(size?.height ?? 720);
    } else {
      await expect(page.getByTestId("pass-reaction-btn")).toHaveCount(0);
    }
    await shot(page, testInfo, `${prefix}-2-card-step-${name}`);
  });
}
