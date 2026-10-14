import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, holdTurnRedoPost, newTurn } from "./redoMock";

// The new turn is complete: the client asks the server to replay the round by itself, and the
// strip shows that the replay is running.
test("turn redo: replaying the round", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, { ...newTurn, turn_complete: true });
  await holdTurnRedoPost(page);
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toHaveAttribute("data-state", "replaying");
  await expect(bar).toHaveAttribute("aria-busy", "true");
  await shot(page, testInfo, "5-replaying");
});
