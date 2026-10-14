import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// read-only view for another player
test("map picker: read-only view for another player", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6, viewer: "guest_1" });
  await page.getByTestId("lobby-map-button").click();
  await page.getByRole("dialog").waitFor();
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  await expect(page.getByTestId("map-picker-choices")).toHaveCount(0);
  await expect(page.getByTestId("map-reroll")).toHaveCount(0);
  await shot(page, testInfo, "4-readonly-player");
});
