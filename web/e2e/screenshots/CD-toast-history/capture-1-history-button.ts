import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { playerWithHand, opponent } from "../_shared/players";
import { openCornerGame, cornerCrop, publicEntry, pushEntry } from "../G-corner-notifications/corner";

// A toast that was dismissed (or faded) is not gone: the button under the toasts keeps count.
test("history: the button after the toast is gone", async ({ page }, testInfo) => {
  const game = await openCornerGame(page, { players: [playerWithHand(), opponent] });
  pushEntry(game, publicEntry("n1", "other_seat", "played Sabotage", { stage: "reactions" }));
  const toast = page.getByTestId("corner-toast");
  await toast.waitFor();
  await shot(page, testInfo, "1a-toast-shown", cornerCrop(page));
  await toast.click();
  await expect(toast).toHaveCount(0);
  await expect(page.getByTestId("toast-history-toggle")).toContainText("Recent notifications (1)");
  await shot(page, testInfo, "1b-toast-gone-button-left", cornerCrop(page));
});
