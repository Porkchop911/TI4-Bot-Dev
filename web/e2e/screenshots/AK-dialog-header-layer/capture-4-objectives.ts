import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { VIEWPORT, shotAndCheck } from "./_layer";

test("dialog header layer: objectives modal", async ({ page }, testInfo) => {
  await page.setViewportSize(VIEWPORT);
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Imperial: draw a secret"),
  });
  await page.getByTestId("imperial-outcome").waitFor();
  await shotAndCheck(page, testInfo, "4-objectives", page.locator(".objectives-modal-panel"));
});
