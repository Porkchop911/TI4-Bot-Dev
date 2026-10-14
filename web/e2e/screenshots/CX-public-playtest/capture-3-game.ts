import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPublicGame, type SizeName } from "./scene";

for (const size of ["desktop", "phone"] as SizeName[]) {
  test(`feedback button in a game ${size}`, async ({ page }, testInfo) => {
    await openPublicGame(page, size);
    const button = page.getByTestId("feedback-button");
    await expect(button).toBeVisible();
    const box = (await button.boundingBox())!;
    expect(box.height).toBeGreaterThanOrEqual(44);
    const { width, height } = page.viewportSize()!;
    expect(box.x + box.width).toBeLessThanOrEqual(width);
    expect(box.y + box.height).toBeLessThanOrEqual(height);
    // Clear of the turn action bar: the button's bottom edge sits above the bar's top edge.
    const bar = await page.getByTestId("turn-action-bar").boundingBox();
    if (bar) expect(box.y + box.height).toBeLessThanOrEqual(bar.y + 1);
    await shot(page, testInfo, `game-${size}`);
  });
}

test("phone top menu has a Send feedback entry", async ({ page }, testInfo) => {
  await openPublicGame(page, "phone");
  await page.getByTestId("top-menu-button").click();
  const entry = page.getByTestId("feedback-menu-button");
  await expect(entry).toBeVisible();
  expect((await entry.boundingBox())!.height).toBeGreaterThanOrEqual(44);
  await shot(page, testInfo, "phone-menu");
  await entry.click();
  await expect(page.getByTestId("feedback-sheet")).toBeVisible();
});
