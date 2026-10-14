import { expect, test } from "@playwright/test";
import { playUntil } from "../../offlineDriver";
import { shot } from "../_shared/shot";
import { ENGINE_LABEL, SIZES, blockServer, foldDecision, newGame } from "./offlineSaves";

// Feedback written in an offline game goes through the durable queue: queued while there is no
// connection, sent when the server answers, "could not be sent yet" (with Retry) when it refuses.
// The strip names the engine file; so does the build footer of the start screen.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`feedback states and engine label ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(180_000);
    await page.setViewportSize(viewport);
    await blockServer(page, { health: true });
    let answer: "down" | "ok" | "refuse" = "down";
    await page.route("**/api/feedback", (route) => {
      if (answer === "down") return route.abort();
      if (answer === "refuse") return route.fulfill({ status: 400, json: { error: "invalid_feedback" } });
      return route.fulfill({ json: { id: "fb_demo", duplicate: false } });
    });
    const id = await newGame(page, { players: 4, seat: 3, seed: 11 });
    await playUntil(page, id, 6);
    await foldDecision(page);
    const send = async (text: string) => {
      await page.evaluate(() => window.dispatchEvent(new Event("ti4:open-feedback")));
      await page.getByTestId("feedback-text").fill(text);
      await page.getByTestId("feedback-submit").click();
      await expect(page.getByTestId("feedback-queued")).toBeVisible();
      return page.getByTestId("feedback-done");
    };
    await expect(page.getByTestId("offline-engine")).toHaveText(ENGINE_LABEL);

    // 1. No connection: queued, and the game goes on.
    await (await send("The bot took the wrong planet.")).dispatchEvent("click");
    await foldDecision(page);
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("will be sent");
    await shot(page, testInfo, `1-queued-${size}`);

    // 2. The server answers: sent.
    answer = "ok";
    await page.evaluate(() => window.dispatchEvent(new Event("online")));
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("1 feedback report sent");
    await shot(page, testInfo, `2-sent-${size}`);

    // 3. The server refuses a report: it says so and offers Retry.
    answer = "refuse";
    await (await send("Second report, refused by the server.")).dispatchEvent("click");
    await foldDecision(page);
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("could not be sent");
    await shot(page, testInfo, `3-failed-with-retry-${size}`);
    answer = "ok";
    await page.getByTestId("offline-feedback-retry").dispatchEvent("click");
    await expect(page.getByTestId("offline-feedback-chip")).toContainText("2 feedback reports sent");

    // 4. The start screen's footer names the engine of this page too (the public build also names its commit).
    await shot(page, testInfo, `4-game-with-engine-label-${size}`);
    await page.goBack(); // in-page navigation: the engine stays loaded in this page
    await page.getByTestId("offline-back").click();
    await expect(page.getByTestId("build-footer-engine")).toBeVisible();
    await shot(page, testInfo, `5-build-footer-${size}`);
  });
}
