import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openPickerOn } from "./real";

// phone, 390 px: Won't You Be My Neighbor?
test("community maps: phone layout", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openPickerOn(page, 6, "6pWontYouBeMyNeighbor", "Won't You Be My Neighbor?");
  await shot(page, testInfo, "5-phone-neighbor");
});
