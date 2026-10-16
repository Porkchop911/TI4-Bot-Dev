import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, pushAll, recapAlreadyOn, selection, tacticalTurn } from "./recap";

// The same recap on a phone-width window (the whole screen).
test("recap: phone width", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await recapAlreadyOn(page);
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushAll(game, tacticalTurn("action_60"));
  pushAll(game, [selection("ph-0", "action_70", "third_seat", "tactical")]);
  await page.locator('[data-toast-kind="recap"]').waitFor();
  await shot(page, testInfo, "5-phone");
});
