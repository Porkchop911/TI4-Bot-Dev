import { test } from "@playwright/test";
import { openDesk, answerChoice, shotDesk } from "./_trade";

// The partner's offer in the same two columns, with card text and a net summary, above
// Accept / Refuse / Counter-offer (and the one-counter rule).
test("trade staging desk: answer screen", async ({ page }, testInfo) => {
  await openDesk(page, answerChoice as never, 1000);
  await page.getByTestId("offer-net").waitFor();
  await shotDesk(page, testInfo, "6-answer-screen");
});
