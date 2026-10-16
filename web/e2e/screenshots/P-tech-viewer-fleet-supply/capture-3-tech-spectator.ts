import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryPlayers } from "../_shared/fixtures";

// A spectator has no faction: the modal shows the tech tree and every player's markers, no faction section.
test("technologies modal: spectator gets no faction section", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: galleryPlayers, spectator: true, choice: null });
  await page.getByTestId("technology-modal-button").click();
  await page.getByTestId("technology-modal").waitFor();
  await shot(page, testInfo, "3-tech-spectator", { of: page.getByTestId("technology-modal"), pad: 4 });
});
