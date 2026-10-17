import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { pushEntry, publicEntry } from "../G-corner-notifications/corner";
import { openScene } from "./scene";

// A spectator has the bell too, with every seat's actions.
test("spectator: bell and list (phone)", async ({ page }, testInfo) => {
  await openScene(page, "390x844", { spectator: true });
  await expect(page.getByTestId("toast-history-mobile-toggle")).toBeVisible();
  await page.getByTestId("toast-history-mobile-toggle").click();
  await expect(page.getByTestId("toast-history-item").first()).toBeVisible();
  await expect(page.getByTestId("toast-history-panel")).toContainText("Mira");
  await shot(page, testInfo, "spectator-390x844");
});

// The same log entry delivered again (a resync) never adds a second line.
test("an entry delivered twice is listed once", async ({ page }) => {
  const game = await openScene(page, "390x844");
  for (const id of ["t1", "t2", "t3", "t4"]) pushEntry(game, publicEntry(id, id === "t1" || id === "t3" ? "other_seat" : "third_seat", "again"));
  await page.waitForTimeout(500);
  await page.getByTestId("toast-history-mobile-toggle").click();
  await expect(page.getByTestId("toast-history-item")).toHaveCount(4);
});
