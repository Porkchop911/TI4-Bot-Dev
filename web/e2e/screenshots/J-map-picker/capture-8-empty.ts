import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// empty state: seven players, no ready-made layout, and Random is rejected
test("map picker: empty state", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 7, alias: null, rejectRandom: true });
  await page.getByTestId("lobby-map-button").click();
  await expect(page.getByTestId("map-picker-empty")).toBeVisible();
  await page.getByTestId("map-reroll").click();
  await shot(page, testInfo, "8-empty");
});
