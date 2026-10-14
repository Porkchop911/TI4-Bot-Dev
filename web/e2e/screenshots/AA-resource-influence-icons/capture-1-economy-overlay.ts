import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { hoverShot } from "../_shared/hover";
import { openMockedGame } from "../_shared/mockGame";
import { systemActivationOptions } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";

const choice = { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions };

// The economy overlay: each system's ready resources and influence as icon + number on the map.
test("economy overlay on the map", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice });
  await page.getByTestId("overlay-btn-economy").click();
  await page.getByTestId("system-activation-bar").waitFor();
  await shot(page, testInfo, "1a-economy-overlay");
});

// Hovering a system with the overlay on: ready / total / exhausted lines with the same icons.
test("economy overlay tooltip", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice });
  await page.getByTestId("overlay-btn-economy").click();
  await hoverShot(page, testInfo, "1b-economy-tooltip", page.getByTestId("system-hex-26"), {
    tooltip: page.getByTestId("system-tooltip"),
  });
});

// The toolbar button for the overlay is now the two icons (accessible name: resources and influence overlay).
test("economy overlay toolbar button", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice });
  await shot(page, testInfo, "1c-toolbar-button", { of: page.getByLabel("Map Overlays"), pad: 12 });
});
