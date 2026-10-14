import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// host view, Standard selected, 6 players
test("map picker: host view, Standard selected, 6 players", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6 });
  await page.getByTestId("lobby-map-button").click();
  const picker = page.getByRole("dialog");
  await picker.waitFor();
  await expect(page.getByTestId("map-card-6pStandard")).toBeVisible();
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  await shot(page, testInfo, "1-host-standard");
});
