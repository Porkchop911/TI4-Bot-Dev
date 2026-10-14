import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// phone layout, 390 px wide
test("map picker: phone layout", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMapLobby(page, { players: 6 });
  await page.getByTestId("lobby-map-button").click();
  await page.getByRole("dialog").waitFor();
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  await shot(page, testInfo, "5-phone");
});
