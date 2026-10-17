import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPickerOn } from "../CO-community-maps/real";

// The picker's strip cards and its large board both draw the lanes.
test("map picker: a hyperlane map, desktop", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const card = await openPickerOn(page, 5, "5pRaceForMecatol", "Race for Mecatol");
  await expect(card).toContainText("Hyperlanes");
  await expect(page.getByTestId("map-preview-board").getByTestId("hyperlane-network").first()).toBeVisible();
  await shot(page, testInfo, "7-picker-desktop");
});

test("map picker: a hyperlane map, phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPickerOn(page, 5, "5pLearnToPlayPoK", "Learn to Play (PoK)");
  await shot(page, testInfo, "7-picker-390x844");
});
