import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, holdTurnRedoPost } from "./redoMock";

// While the server rewinds, the strip says so; the button waits.
test("turn redo: rewinding", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, null);
  await holdTurnRedoPost(page);
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    phase: "action",
  });
  await openLogFully(page);
  await page.getByTestId("turn-redo-btn").click();
  await expect(page.getByTestId("turn-redo-bar")).toHaveAttribute("data-state", "rewinding");
  await expect(page.getByTestId("turn-redo-btn")).toBeDisabled();
  await shot(page, testInfo, "3-rewinding");
});
