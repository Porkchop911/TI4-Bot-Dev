import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

// An Elect Planet agenda: the outcomes are planets, so they are voted for on the map.
test("planet selection: vote for a planet", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    phase: "agenda",
    round: 3,
    players: [playerWithHand(), opponent],
    choice: galleryDecision("Elect Planet vote"),
  });
  await page.getByTestId("planet-selection-bar").waitFor();
  await page.getByTestId("system-hex-26").click();
  await shot(page, testInfo, "4-planet-outcome-vote");
});
