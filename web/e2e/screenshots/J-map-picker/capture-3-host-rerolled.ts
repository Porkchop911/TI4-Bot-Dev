import { test, expect } from "@playwright/test";
import { openMapLobby } from "../_shared/mapLobby";
import { shot } from "../_shared/shot";

// host view after Re-roll shows a different board
test("map picker: host view after Re-roll shows a different board", async ({ page }, testInfo) => {
  await openMapLobby(page, { players: 6 });
  await page.getByTestId("lobby-map-button").click();
  const picker = page.getByRole("dialog");
  await picker.waitFor();
  await picker.getByTestId("map-card-random").click();
  await expect(page.getByTestId("map-picker-summary")).toContainText(/random/i);
  const board = page.getByTestId("map-preview-board");
  const before = await board.innerHTML();
  await page.getByTestId("map-reroll").click();
  await expect.poll(async () => (await board.innerHTML()) !== before).toBe(true);
  await expect(page.getByTestId("map-reroll")).toHaveText("Re-roll");
  await shot(page, testInfo, "3-host-rerolled");
});
