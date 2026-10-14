import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { solSeat, opponent } from "./fixtures";

test.use({ viewport: { width: 390, height: 844 }, hasTouch: true, isMobile: true });

test("faction info: phone", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [solSeat(), opponent] });
  // On a phone the player sheet lives in a drawer.
  await page.getByTestId("player-sheet-toggle").tap();
  const button = page.getByTestId("faction-info-button").first();
  await button.scrollIntoViewIfNeeded();
  await button.tap();
  await page.getByTestId("faction-info-card").waitFor();
  await shot(page, testInfo, "7-faction-card-phone");
});
