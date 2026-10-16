import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, pushAll, recapAlreadyOn, selection, OTHER } from "./recap";

// A pass is a whole turn: its recap appears at once.
test("recap: pass", async ({ page }, testInfo) => {
  await recapAlreadyOn(page);
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushAll(game, [selection("p-0", "action_50", OTHER, "pass")]);
  await page.locator('[data-toast-kind="recap"]').waitFor();
  await shot(page, testInfo, "4-pass", cornerCrop(page));
});
