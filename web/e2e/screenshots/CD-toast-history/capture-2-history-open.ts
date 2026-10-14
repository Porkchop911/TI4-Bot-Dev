import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, publicEntry, pushEntry } from "../G-corner-notifications/corner";

// The list shows every toast of the session again, newest first.
test("history: the opened list", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  const steps: [string, string][] = [
    ["played Sabotage", "reactions"],
    ["scored objective expand_borders", "status"],
    ["researched Neural Motivator", "strategy"],
    ["activated #27", "activation"],
    ["picked Warfare", "draft"],
  ];
  for (const [i, [what, stage]] of steps.entries()) {
    pushEntry(game, publicEntry(`h${i}`, "other_seat", what, { stage }));
    await page.waitForTimeout(150);
  }
  // Let the toasts go, as they would once read.
  while ((await page.getByTestId("corner-toast").count()) > 0) {
    await page.getByTestId("corner-toast").last().click();
    await page.waitForTimeout(400);
  }
  await expect(page.getByTestId("corner-toast")).toHaveCount(0);
  await page.getByTestId("toast-history-toggle").click();
  await expect(page.getByTestId("toast-history-item")).toHaveCount(steps.length);
  await shot(page, testInfo, "2-history-open", cornerCrop(page));
});
