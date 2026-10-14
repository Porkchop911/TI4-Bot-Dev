import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, conflict } from "./redoMock";

// A recorded decision no longer fits: the seat the engine asks decides live; the original can come back.
test("turn redo: conflict", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, conflict);
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toHaveAttribute("data-state", "conflict");
  await expect(page.getByTestId("turn-redo-keep")).toHaveText("Continue from here");
  await shot(page, testInfo, "7-conflict");
});
