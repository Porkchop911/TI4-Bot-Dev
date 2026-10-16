import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus } from "./redoMock";

// The host can also pick another seat; the button then says whose turn goes back.
test("turn redo: controls for the host", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, null);
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    phase: "status",
  });
  await openLogFully(page);
  await page.getByTestId("turn-redo-seat").selectOption({ index: 1 });
  await page.getByTestId("turn-redo-turns").selectOption("2");
  await expect(page.getByTestId("turn-redo-btn")).toHaveText("Redo their last turn");
  await shot(page, testInfo, "2-controls-host", { of: page.locator("#event-log-drawer"), pad: 8 });
});
