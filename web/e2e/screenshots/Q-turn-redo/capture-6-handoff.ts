import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, handoff } from "./redoMock";

// Auto-play reached the start of the redoing seat's next turn: control is back, the original is still restorable.
test("turn redo: back to the seat", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, handoff);
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toHaveAttribute("data-state", "handoff");
  await expect(bar).toContainText("next turn begins");
  await shot(page, testInfo, "6-handoff");
});
