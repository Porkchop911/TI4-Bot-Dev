import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// error state: the preview request failed
test("map picker: error state", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, preview: "error" });
  await page.getByTestId("lobby-map-button").click();
  await expect(page.getByTestId("map-preview-error")).toBeVisible();
  await shot(page, testInfo, "7-error");
});
