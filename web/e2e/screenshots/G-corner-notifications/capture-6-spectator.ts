import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, publicEntry, pushEntry } from "./corner";

// A spectator has no seat of their own, so every player's actions get a toast.
test("notifications: spectator", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent], spectator: true });
  pushEntry(game, publicEntry("s1", "other_seat", "played Direct Hit"));
  pushEntry(game, publicEntry("s2", playerWithHand().id, "activated #22"));
  await page.getByTestId("corner-toast").nth(1).waitFor();
  await shot(page, testInfo, "6-spectator", cornerCrop(page));
});
