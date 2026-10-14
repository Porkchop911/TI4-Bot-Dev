import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { hoverShot } from "../_shared/hover";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";

// No decision pending: clicking a system opens the inspector; its planets list resources and influence as icons.
test("system inspector", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: null });
  await page.getByTestId("system-hex-26").click();
  const inspector = page.getByTestId("system-inspector");
  await inspector.waitFor();
  await shot(page, testInfo, "6a-system-inspector", { of: inspector, pad: 8 });
});

// Hovering a tile lists its planets with the same icons.
test("board tile tooltip", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: null });
  await hoverShot(page, testInfo, "6b-tile-tooltip", page.getByTestId("system-hex-26"), {
    tooltip: page.getByTestId("system-tooltip"),
  });
});

// The player sheet badges: ready / total, now from the shared component.
test("player sheet resource and influence badges", async ({ page }, testInfo) => {
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: null });
  await shot(page, testInfo, "6c-player-stats", { of: page.getByTestId("player-stats-row").first(), pad: 10 });
});

test("system inspector on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, { players: [playerWithHand(), opponent], choice: null });
  await page.getByTestId("system-hex-26").click();
  await page.getByTestId("system-inspector").waitFor();
  await shot(page, testInfo, "6d-inspector-phone");
});
