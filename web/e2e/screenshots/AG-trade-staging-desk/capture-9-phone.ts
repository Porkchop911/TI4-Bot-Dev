import { test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { answerChoice, proposeChoice, tradePlayers, click } from "./_trade";

// Phone width: the columns stack, text stays readable and the action bar sticks to the bottom.
test("trade staging desk: phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, { players: tradePlayers, choice: proposeChoice });
  await page.getByTestId("trade-desk-modal").waitFor();
  for (let i = 0; i < 2; i++) await click(page, "stage-give-tg-inc");
  for (let i = 0; i < 3; i++) await click(page, "stage-receive-tg-inc");
  await page.getByTestId("stage-status-valid").waitFor();
  await shot(page, testInfo, "9-phone-propose");
});

test("trade staging desk: phone answer", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  await openMockedGame(page, { players: tradePlayers, choice: answerChoice as never });
  await page.getByTestId("offer-net").waitFor();
  await shot(page, testInfo, "10-phone-answer");
});
