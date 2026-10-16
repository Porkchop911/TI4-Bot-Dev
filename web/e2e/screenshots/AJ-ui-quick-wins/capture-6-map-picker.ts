import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// The summary line reads "6 players · Standard by ..." instead of the raw alias.
test("map picker: summary line", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6 });
  await page.getByTestId("lobby-map-button").click();
  await page.getByRole("dialog").waitFor();
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  await shot(page, testInfo, "6-map-picker");
});
