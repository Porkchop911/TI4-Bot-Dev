import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { openLogFully } from "../_shared/eventLog";
import { publicEntry } from "../G-corner-notifications/corner";
import { rollEntry, riftDestroyed, riftSurvives, combatRound } from "./dice";

// The event log lists each roll on its own line, where it happened: the faces as chips (hits
// filled), the reason, the system and what it did. Nothing in it can be undone.
test("event log: gravity rift and combat rolls", async ({ page }, testInfo) => {
  const move = (id: string, seat: string, what: string, cursor: number) =>
    publicEntry(id, seat, what, {
      action_id: "action_1",
      action_type: "tactical",
      action_actor: seat,
      stage: "movement",
      decision_count: cursor,
    });
  const events = [
    publicEntry("game-1", opponent.id, "began a tactical action", { action_id: "action_1", action_type: "tactical", action_actor: opponent.id, stage: "action selection", decision_count: 1 }),
    move("game-2", opponent.id, "activated #01", 2),
    move("game-3", opponent.id, "loaded mahact_infantry", 3),
    rollEntry(riftSurvives(opponent.id), { id: "game-4", decision_count: 3 }),
    rollEntry(riftDestroyed(actor), { id: "game-5", decision_count: 3 }),
    move("game-6", opponent.id, "Done moving", 4),
    rollEntry(combatRound(opponent.id, [9, 3, 7], 2), { id: "game-7", decision_count: 4 }),
    rollEntry(combatRound(actor, [4, 1], 0), { id: "game-8", decision_count: 4 }),
    rollEntry({ ...combatRound(actor, [8, 1], 1), faces: [8, 10], rerolled: [1], outcome: "2 hits", hits: 2 }, { id: "game-9", decision_count: 4 }),
  ];
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events,
    history: { cursor: 4, redo_count: 0 },
  });
  await openLogFully(page);
  const rows = page.getByTestId("event-log-roll");
  await expect(rows).toHaveCount(5);
  await expect(rows.first()).toContainText("Gravity rift in system 67");
  await expect(rows.first()).toContainText("ship survives");
  await expect(rows.nth(1)).toContainText("ship destroyed");
  // Hits are filled, misses dimmed, rerolled dice dashed; and a roll has no Undo button.
  await expect(rows.nth(2).locator('.event-log__die[data-hit="yes"]')).toHaveCount(2);
  await expect(rows.nth(2).locator('.event-log__die[data-hit="no"]')).toHaveCount(1);
  await expect(rows.nth(4).locator('.event-log__die[data-reroll="yes"]')).toHaveCount(1);
  await expect(rows.nth(1).getByRole("button")).toHaveCount(0);
  // The drawer scrolls: show the end of the action, where the rolls are.
  await page.getByTestId("event-log-list").evaluate((el) => {
    el.scrollTop = el.scrollHeight;
  });
  await shot(page, testInfo, "3-event-log-rolls", { of: page.locator("#event-log-drawer"), pad: 8 });
});
