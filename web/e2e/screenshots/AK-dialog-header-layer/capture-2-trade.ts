import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { proposeChoice, tradePlayers } from "../AG-trade-staging-desk/_trade";
import { VIEWPORT, shotAndCheck } from "./_layer";

test("dialog header layer: trade desk", async ({ page }, testInfo) => {
  await page.setViewportSize(VIEWPORT);
  await openMockedGame(page, { players: tradePlayers, choice: proposeChoice });
  await shotAndCheck(page, testInfo, "2-trade", page.getByTestId("trade-desk-modal").locator(".choice-workflow-modal"));
});
