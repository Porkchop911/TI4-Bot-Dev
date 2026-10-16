import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, newTurn } from "./redoMock";

// Rewound: the seat plays the new turn. The way back is one click away.
test("turn redo: playing the new turn", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, newTurn);
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toHaveAttribute("data-state", "new-turn");
  await shot(page, testInfo, "4-new-turn");
});
