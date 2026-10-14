import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryPlayers } from "../_shared/fixtures";

// Before the fix this showed Sol's techs (Player 1's) to every viewer.
test("technologies modal: seat 2 sees its own faction techs", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: galleryPlayers, seat: galleryPlayers[1].id, choice: null });
  await page.getByTestId("technology-modal-button").click();
  await page.getByTestId("faction-technologies-section").waitFor();
  await shot(page, testInfo, "2-tech-seat2", { of: page.getByTestId("technology-modal"), pad: 4 });
});
