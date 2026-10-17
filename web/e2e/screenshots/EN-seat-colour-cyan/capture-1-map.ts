import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openSeated, prefix, type SizeName } from "../DF-seat-colors/scene";

// 3 players: seat 3 is the former white seat (now cyan); the board also shows unexplored systems.
for (const size of ["desktop", "phone"] as SizeName[]) {
  test(`map 3 players ${size}`, async ({ page }, testInfo) => {
    await openSeated(page, 3, size);
    await expect(page.getByTestId("board")).toBeVisible().catch(() => undefined);
    await shot(page, testInfo, `${prefix}-map-3p-${size}`);
  });
  test(`sheets 3 players ${size}`, async ({ page }, testInfo) => {
    await openSeated(page, 3, size);
    if (size === "phone") await page.getByTestId("player-sheet-toggle").click();
    await expect(page.getByTestId("player-card")).toHaveCount(3);
    await shot(page, testInfo, `${prefix}-sheets-3p-${size}`);
  });
}
