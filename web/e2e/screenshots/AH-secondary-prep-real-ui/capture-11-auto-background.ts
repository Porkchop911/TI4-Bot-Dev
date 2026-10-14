import { test, expect } from "@playwright/test";
import { shot } from "../_shared/shot";
import { openWaiting, push, save, secondaryQuestion } from "./prep";

// Auto mode: the decision UI does NOT open; only the non-blocking toast with a short Cancel window.
test("secondary prep: auto mode, decided in the background", async ({ page }, testInfo) => {
  const game = await openWaiting(page, { name: "Technology", card: "pok7technology" });
  await page.getByTestId("secondary-prep-mode-auto").click();
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("secondary-yes-btn").click();
  await save(page);
  push(game, {
    version: 42,
    history: { cursor: 41, redo_count: 0, generation: 0 },
    choice: secondaryQuestion("pok7technology", "spend a strategy token and 4 resources to research", "spend"),
  });
  await page.getByTestId("secondary-autoplay-toast").waitFor();
  await expect(page.getByTestId("pending-choice-dialog")).toHaveCount(0);
  await shot(page, testInfo, "11a-auto-toast-ui-closed");
  // Cancel: the real decision opens for a manual answer.
  await page.getByTestId("secondary-autoplay-cancel").click();
  await page.getByTestId("strategy-secondary-panel").waitFor();
  await shot(page, testInfo, "11b-auto-cancelled-ui-opens");
});
