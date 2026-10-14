import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, publicEntry, pushEntry } from "./corner";

// Another player's public action, from the event log: their name and seat colour, what they did.
test("notifications: another player's action", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushEntry(game, publicEntry("n1", "other_seat", "played Sabotage"));
  await page.getByTestId("corner-toast").waitFor();
  await shot(page, testInfo, "2-other-player-action", cornerCrop(page));
});
