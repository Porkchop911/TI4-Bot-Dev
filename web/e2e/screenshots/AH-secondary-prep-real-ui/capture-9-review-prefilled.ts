import { test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, push, save, secondaryQuestion } from "./prep";

// Review mode (the default): the real question opens with the prepared answer marked in the real panel.
test("secondary prep: review mode, prefilled", async ({ page }, testInfo) => {
  const game = await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await save(page);
  push(game, {
    version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    choice: secondaryQuestion("pok7technology", "spend a strategy token and 4 resources to research", "spend"),
  });
  await page.getByTestId("secondary-prepared-bar").waitFor();
  await shot(page, testInfo, "9-review-prefilled");
});
