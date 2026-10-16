import { test } from "@playwright/test";
import { bombardmentStep, defendersAfterBombardment, invasionBoard, openInvasion, take, viewports } from "./scene";

// The invasion panel right after the bombardment step.
for (const v of viewports) {
  test(`bombardment-result-${v.id}`, async ({ page }, testInfo) => {
    const board = invasionBoard({ phase: "commit_ground_forces", units: defendersAfterBombardment, step: bombardmentStep });
    await openInvasion(page, v, board, null);
    await page.getByTestId("invasion-step").scrollIntoViewIfNeeded();
    await take(page, testInfo, "bombardment-result", v);
  });
}
