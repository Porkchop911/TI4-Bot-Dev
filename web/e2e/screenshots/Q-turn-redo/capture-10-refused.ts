import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, refuseTurnRedoPost } from "./redoMock";

// The server refuses (here: the seat has no turn to redo); the reason shows in the strip and nothing changed.
test("turn redo: refused", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, null);
  await refuseTurnRedoPost(page, 400, "Invalid history target: gallery_seat has no turn in this game to redo");
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  await openLogFully(page);
  await page.getByTestId("turn-redo-btn").click();
  await expect(page.getByTestId("turn-redo-error")).toBeVisible();
  await shot(page, testInfo, "10-refused");
});
