import { test } from "@playwright/test";
import { fightChoice, invasionBoard, openInvasion, spaceCannonStep, take, viewports } from "./scene";

// The invasion panel right after the Space Cannon Defense step, before the first ground round.
for (const v of viewports) {
  test(`space-cannon-result-${v.id}`, async ({ page }, testInfo) => {
    const units = spaceCannonStep.after;
    const board = invasionBoard({ phase: "ground_battle", units, step: spaceCannonStep });
    await openInvasion(page, v, board, fightChoice);
    await page.getByTestId("invasion-step").scrollIntoViewIfNeeded();
    await take(page, testInfo, "space-cannon-result", v);
  });
}
