import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, pushAll, recapAlreadyOn, selection, tacticalTurn, OTHER } from "./recap";

// A tactical action with movement, combat and production: no toast while it plays, one recap when
// the next player starts their action.
test("recap: tactical action with combat and production", async ({ page }, testInfo) => {
  await recapAlreadyOn(page);
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushAll(game, tacticalTurn("action_10"));
  await page.waitForTimeout(500);
  await expect(page.getByTestId("corner-toast")).toHaveCount(0);
  pushAll(game, [selection("n-0", "action_20", "third_seat", "tactical")]);
  await page.locator('[data-toast-kind="recap"]').waitFor();
  expect(OTHER).toBe("other_seat");
  await shot(page, testInfo, "2-tactical", cornerCrop(page));
});
