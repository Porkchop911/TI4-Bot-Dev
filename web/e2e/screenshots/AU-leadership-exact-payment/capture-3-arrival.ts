import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { acceptingScript, leadershipWindow, openScene, prefix, prepareAndSave, pushChoice } from "./scene";

// Arrival (Review): the real question opens. After: the usual panel has the purchase staged (one
// token, fleet pool) and the map marks the payment that was chosen (Jord + Quann); the bar says
// what Confirm sends. Before: the bar named no payment and the panel opened empty, so the payment
// was Auto-pay's (Lodor) unless the player redid it.
test("leadership prep: arrival with the prepared payment selected", async ({ page }, testInfo) => {
  const game = await openScene(page, acceptingScript);
  await prepareAndSave(page);
  pushChoice(game, 42, leadershipWindow());
  await page.getByTestId("secondary-prepared-bar").waitFor();
  await page.waitForTimeout(300);
  await shot(page, testInfo, `${prefix}-3-arrival-review`);
});
