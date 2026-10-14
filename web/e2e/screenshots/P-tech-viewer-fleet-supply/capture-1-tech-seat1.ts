import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryPlayers } from "../_shared/fixtures";

// The header Technologies button, opened by the first seat (Sol): Sol's faction techs.
test("technologies modal: seat 1 sees its own faction techs", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: galleryPlayers, choice: null });
  await page.getByTestId("technology-modal-button").click();
  await page.getByTestId("faction-technologies-section").waitFor();
  await shot(page, testInfo, "1-tech-seat1", { of: page.getByTestId("technology-modal"), pad: 4 });
});
