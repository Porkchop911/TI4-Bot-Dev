import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, publicEntry, pushEntry } from "./corner";

// The mute button in the player sheet header: with notifications off, other players' actions show
// no toast (the choice is remembered in this browser).
test("notifications: muted", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  await page.getByTestId("toast-mute-btn").click();
  pushEntry(game, publicEntry("m1", "other_seat", "played Sabotage"));
  await page.waitForTimeout(600);
  await shot(page, testInfo, "5-muted", { of: page.getByTestId("player-sheet-panel").locator("h2").locator(".."), pad: 40 });
});
