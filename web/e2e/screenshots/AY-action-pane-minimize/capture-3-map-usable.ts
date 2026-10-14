import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { barBox, openTurnMenu, prefix } from "./scene";

// With the pane minimized the map above it is not covered: the board's visible bottom edge moves
// down by the height gained. (The expanded shot of the same scene is capture 1.)
test.skip(prefix === "before", "the control does not exist on the base commit");
test("map area gained when minimized at 390x844", async ({ page }, testInfo) => {
  await openTurnMenu(page, 390, 844);
  const expanded = await barBox(page);
  await page.getByTestId("turn-bar-minimize").click();
  const collapsed = await barBox(page);
  expect(collapsed.y).toBeGreaterThan(expanded.y + 100);
  const board = await page.getByTestId("ti4-board-svg").boundingBox();
  console.log(`MEASURE board visible above bar: expanded=${Math.round(expanded.y - board!.y)} collapsed=${Math.round(collapsed.y - board!.y)}`);
  await shot(page, testInfo, `${prefix}-map-usable`);
});
