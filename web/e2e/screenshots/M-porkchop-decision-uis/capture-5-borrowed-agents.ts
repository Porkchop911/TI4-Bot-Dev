import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// Yssaril's Ssruu copying another faction's agent.
test("porkchop decisions: Ssruu round agent unit", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Ssruu: choose a unit for the copied agent"),
  });
  await page.getByTestId("offer-card-panel").waitFor();
  await shot(page, testInfo, "12-ssruu-round", { of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"), pad: 8 });
});

// Ssruu copying I48S: the options now carry their planet, so the planet bar takes over.
test("porkchop decisions: L1Z1X copy planet", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: galleryDecision("L1Z1X agent (copied): replace which infantry"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await shot(page, testInfo, "13-l1z1x-copy", { of: page.getByTestId("planet-selection-bar"), pad: 12 });
});
