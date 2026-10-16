import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPublicLobby, type SizeName } from "./scene";

for (const size of ["desktop", "phone"] as SizeName[]) {
  test(`alpha notice and feedback button in the lobby ${size}`, async ({ page }, testInfo) => {
    await openPublicLobby(page, size);
    await expect(page.getByTestId("alpha-notice")).toBeVisible();
    await expect(page.getByTestId("alpha-notice")).toContainText("Alpha");
    await expect(page.getByTestId("build-footer")).toContainText("build 51390594");
    await expect(page.getByTestId("build-footer")).toContainText("protocol v3");
    const button = page.getByTestId("feedback-button");
    await expect(button).toBeVisible();
    const box = (await button.boundingBox())!;
    expect(box.height).toBeGreaterThanOrEqual(44);
    const { width, height } = page.viewportSize()!;
    expect(box.x + box.width).toBeLessThanOrEqual(width);
    expect(box.y + box.height).toBeLessThanOrEqual(height);
    await shot(page, testInfo, `lobby-${size}`);
  });
}
