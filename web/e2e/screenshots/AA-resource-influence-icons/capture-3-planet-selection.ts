import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// A card asks for a planet: candidate chips and, once chosen, the action line carry the planet's values as icons.
test("planet selection bar", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: galleryDecision("planet selection") });
  const bar = page.getByTestId("planet-selection-bar");
  await bar.waitFor();
  await shot(page, testInfo, "3a-planet-chips", { of: bar, pad: 8 });
  await page.getByTestId("system-hex-26").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await shot(page, testInfo, "3b-planet-chosen", { of: bar, pad: 8 });
});

test("planet selection bar on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: galleryDecision("planet selection") });
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("system-hex-26").click();
  await page.getByTestId("planet-selection-facts").waitFor();
  await shot(page, testInfo, "3c-planet-chosen-phone");
});
