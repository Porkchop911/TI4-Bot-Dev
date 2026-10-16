import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, pushAll, recapAlreadyOn, selection, strategicTurn } from "./recap";

// A strategic action, summed up by what the public log shows of it.
test("recap: strategic action", async ({ page }, testInfo) => {
  await recapAlreadyOn(page);
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushAll(game, strategicTurn("action_30"));
  pushAll(game, [selection("n-1", "action_40", "third_seat", "pass")]);
  await page.locator('[data-toast-kind="recap"]').first().waitFor();
  await shot(page, testInfo, "3-strategic", cornerCrop(page));
});
