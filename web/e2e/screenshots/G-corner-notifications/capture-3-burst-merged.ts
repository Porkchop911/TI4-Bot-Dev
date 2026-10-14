import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, publicEntry, pushEntry, thirdPlayer } from "./corner";

// A burst becomes a few toasts: three moves by one player are one "moved 3 units" line, and the
// stack is capped at three, one line per player and kind of action.
test("notifications: a burst is merged", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent, thirdPlayer] });
  pushEntry(game, publicEntry("b1", "other_seat", "activated #22"));
  pushEntry(game, publicEntry("b2", "other_seat", "moved cruiser from #19 to #22"));
  pushEntry(game, publicEntry("b3", "other_seat", "moved carrier from #19 to #22"));
  pushEntry(game, publicEntry("b4", "other_seat", "moved destroyer from #20 to #22"));
  pushEntry(game, publicEntry("b5", "third_seat", "played Sabotage"));
  pushEntry(game, publicEntry("b6", "third_seat", "played Sabotage"));
  await page.getByTestId("corner-toast").nth(2).waitFor();
  await shot(page, testInfo, "3-burst-merged", cornerCrop(page));
});
