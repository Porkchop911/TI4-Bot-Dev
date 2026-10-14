import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// host view, Random selected with Re-roll
test("map picker: host view, Random selected with Re-roll", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6 });
  await page.getByTestId("lobby-map-button").click();
  const picker = page.getByRole("dialog");
  await picker.waitFor();
  await picker.getByTestId("map-card-random").click();
  await expect(page.getByTestId("map-preview-board")).toBeVisible();
  await expect(page.getByTestId("map-picker-summary")).toContainText(/random/i);
  await expect(page.getByTestId("map-reroll")).toBeEnabled();
  await shot(page, testInfo, "2-host-random");
});
