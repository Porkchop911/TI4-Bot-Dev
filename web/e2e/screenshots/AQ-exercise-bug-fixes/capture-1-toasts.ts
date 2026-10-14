import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, pushAll, recapAlreadyOn, selection, tacticalTurn } from "../X-turn-recap-toast/recap";
import { publicEntry, pushEntry } from "../G-corner-notifications/corner";
import { prefix, record } from "./shared";

// Bug 2: the server sends `visibility` as an object ({"visibility":"public"}); the client compared it
// with the string "public", so no action toast and no recap toast ever fired in a real game. These
// events use the real wire shape (see crates/ti4-server/fixtures/event_visibility.json).
// Regenerate the "before" shots with TI4_SHOT_PREFIX=before on the base commit.

test("toasts with the real wire shape: another player's action", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushEntry(game, publicEntry("n1", "other_seat", "played Sabotage"));
  const shown = await page
    .getByTestId("corner-toast")
    .waitFor({ timeout: prefix === "before" ? 1500 : 5000 })
    .then(() => true, () => false);
  record("action-toast-shown", shown);
  await shot(page, testInfo, `${prefix}-1-action-toast`, cornerCrop(page));
});

test("toasts with the real wire shape: turn recap", async ({ page }, testInfo) => {
  await recapAlreadyOn(page);
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushAll(game, tacticalTurn("action_10"));
  await page.waitForTimeout(400);
  pushAll(game, [selection("n-0", "action_20", "third_seat", "tactical")]);
  const shown = await page
    .locator('[data-toast-kind="recap"]')
    .waitFor({ timeout: prefix === "before" ? 1500 : 5000 })
    .then(() => true, () => false);
  record("recap-toast-shown", shown);
  await shot(page, testInfo, `${prefix}-2-recap-toast`, cornerCrop(page));
});
