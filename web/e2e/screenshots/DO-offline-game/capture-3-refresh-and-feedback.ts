import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playUntil, storedAnswers } from "../../offlineDriver";
import { startGame } from "./offlineGame";

// A refresh in the middle of a game plays the stored answers again and lands on the same
// question; feedback sent from the game is queued (no server), with the replay data attached.
test("refresh resumes the game and feedback is queued", async ({ page }, testInfo) => {
  test.setTimeout(180_000);
  const id = await startGame(page);
  const answers = await playUntil(page, id, 14);
  const nonce = await page.getByTestId("pending-choice-dialog").count();
  await page.reload();
  await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 60_000 });
  expect(page.url()).toContain(id);
  expect(await storedAnswers(page, id)).toBeGreaterThanOrEqual(answers);
  expect(nonce).toBeGreaterThanOrEqual(0);
  await shot(page, testInfo, "5-resumed-after-refresh-desktop");

  // The same event the phone menu dispatches (the floating button can sit under a decision dialog).
  await page.evaluate(() => window.dispatchEvent(new Event("ti4:open-feedback")));
  await page.getByTestId("feedback-text").fill("The bot took the wrong planet.");
  await shot(page, testInfo, "6-feedback-sheet-desktop");
  await page.getByTestId("feedback-submit").click();
  await expect(page.getByTestId("feedback-queued")).toHaveText("Will be sent");
  await shot(page, testInfo, "7-feedback-queued-desktop");
});
