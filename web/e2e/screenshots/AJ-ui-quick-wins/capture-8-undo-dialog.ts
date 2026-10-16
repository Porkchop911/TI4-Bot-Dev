import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { openLogFully } from "../_shared/eventLog";
import { playerWithHand, opponent } from "../_shared/players";

// Undo from the event log asks once, in the game's own dialog, and says it affects everyone.
test("undo: confirm dialog", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    history: { cursor: 5, redo_count: 0 },
  });
  await openLogFully(page);
  await page.getByRole("button", { name: /Undo from decision/ }).first().click();
  await page.getByTestId("undo-confirm-dialog").waitFor();
  await shot(page, testInfo, "8-undo-dialog");
});
