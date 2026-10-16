import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, OTHER } from "./redoMock";

// A seated player (not the host) finds "Redo my last turn" in the open event log, next to Copy replay.
test("turn redo: controls for a player", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, null);
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    phase: "status",
    host: OTHER,
  });
  await openLogFully(page);
  await expect(page.getByTestId("turn-redo-btn")).toHaveText("Redo my last turn");
  await expect(page.getByTestId("turn-redo-seat")).toHaveCount(0);
  await shot(page, testInfo, "1-controls", { of: page.locator("#event-log-drawer"), pad: 8 });
});
