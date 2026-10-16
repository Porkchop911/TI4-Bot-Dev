import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openScene } from "../BD-mobile-top-chrome/scene";

/** `before` is taken on the commit without the button. */
const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

for (const size of ["390x844", "360x740"] as const) {
  test(`bottom buttons ${size}`, async ({ page }, testInfo) => {
    await openScene(page, size, "waiting");
    if (prefix === "after") {
      const button = page.getByTestId("objectives-mobile-button");
      await expect(button).toBeVisible();
      const box = (await button.boundingBox())!;
      expect(box.height).toBeGreaterThanOrEqual(36);
      expect(box.x + box.width).toBeLessThanOrEqual(page.viewportSize()!.width);
    }
    await shot(page, testInfo, `${prefix}-bottom-${size}`);
  });
}

test("objectives button opens the dialog 390x844", async ({ page }, testInfo) => {
  await openScene(page, "390x844", "waiting");
  if (prefix === "before") return;
  await page.getByTestId("objectives-mobile-button").click();
  await expect(page.getByRole("dialog")).toBeVisible();
  await shot(page, testInfo, `${prefix}-opened-390x844`);
});
