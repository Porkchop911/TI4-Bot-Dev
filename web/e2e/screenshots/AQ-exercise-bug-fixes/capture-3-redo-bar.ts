import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame, GAME_ID } from "../_shared/mockGame";
import { actionCardEventLog } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { routeTurnRedoStatus, holdTurnRedoPost, newTurn, handoff } from "../Q-turn-redo/redoMock";
import { prefix, record } from "./shared";

// Bug 5: the redo bar flickered. Causes: the status fetch ran again on every render (the session
// hook handed out new callbacks each render, so every response re-rendered the app and fetched
// again), and Restore left the bar while a command was busy. Measured here: status requests in a
// still 2 s window, and whether Restore stays in the bar while the replay runs.

async function open(page: import("@playwright/test").Page) {
  await openMockedGame(page, { players: [playerWithHand(), opponent], events: actionCardEventLog(), phase: "action" });
}

test("redo bar: status requests while nothing changes", async ({ page }, testInfo) => {
  let gets = 0;
  await page.route(`**/api/games/${GAME_ID}/turn-redo`, (route) => {
    if (route.request().method() === "GET") {
      gets++;
      return route.fulfill({ json: { status: handoff } });
    }
    return route.fallback();
  });
  await open(page);
  await expect(page.getByTestId("turn-redo-bar")).toHaveAttribute("data-state", "handoff");
  gets = 0;
  await page.waitForTimeout(2000);
  record("status-requests-in-2s-idle", gets);
  await shot(page, testInfo, `${prefix}-4-redo-bar-handoff`);
});

test("redo bar: Restore stays while the round replays", async ({ page }, testInfo) => {
  await routeTurnRedoStatus(page, { ...newTurn, turn_complete: true });
  await holdTurnRedoPost(page);
  await open(page);
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toHaveAttribute("data-state", "replaying");
  await expect(bar).toHaveAttribute("aria-busy", "true");
  await page.waitForTimeout(300);
  const restore = await page.getByTestId("turn-redo-restore").count();
  record("restore-button-while-replay-runs", restore === 1);
  await shot(page, testInfo, `${prefix}-5-redo-bar-replaying`);
});
