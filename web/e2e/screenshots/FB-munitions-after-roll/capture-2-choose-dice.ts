import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openMockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { perDieQuestion } from "./scene";

// After "choose which missed dice": each missed die is asked about, with the whole roll shown.
test("munitions: choose dice, first miss", async ({ page }, testInfo) => {
  await openMockedGame(page, {
    players: [playerWithHand({ trade_goods: 3 }), opponent],
    choice: perDieQuestion(0) as never,
  });
  await page.getByTestId("pending-choice-dialog").waitFor();
  await shot(page, testInfo, "3-choose-first-miss", {
    of: page.getByTestId("pending-choice-dialog").locator(".choice-dialog__panel"),
    pad: 8,
  });
});
