import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { routeTurnRedoStatus, handoff, OTHER } from "./redoMock";

// Everyone else at the table sees what is happening, without the buttons.
test("turn redo: another player's view", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, {
    ...handoff,
    can_control: false,
    seat: OTHER,
    outcome: { ...handoff.outcome!, stop: { kind: "handoff", seat: OTHER } },
  });
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action", host: OTHER });
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toBeVisible();
  await expect(page.getByTestId("turn-redo-restore")).toHaveCount(0);
  await shot(page, testInfo, "9-watching");
});
