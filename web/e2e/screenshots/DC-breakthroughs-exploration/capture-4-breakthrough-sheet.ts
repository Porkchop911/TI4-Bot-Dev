import { expect, test } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { shot } from "../_shared/shot";
import { board, players } from "./_fixtures";

// The player sheet: the viewer's unlocked breakthrough in full, a rival's still locked, and the faction card.
test("player sheet breakthrough rows", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { list, state } = players();
  await openMockedGame(page, { board, players: list, state });
  const rows = page.getByTestId("player-breakthrough");
  await expect(rows).toHaveCount(2);
  await expect(rows.nth(0)).toHaveAttribute("data-unlocked", "true");
  await expect(rows.nth(1)).toHaveAttribute("data-unlocked", "false");
  await shot(page, testInfo, "sheet-desktop", { of: page.locator('[data-testid="player-card"]').first().locator(".."), pad: 12 });
});

test("faction card breakthrough section", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 1440, height: 900 });
  const { list, state } = players();
  await openMockedGame(page, { board, players: list, state });
  await page.getByTestId("faction-info-button").first().click();
  const section = page.getByTestId("faction-breakthrough");
  await section.waitFor();
  await expect(section).toContainText("unlocked");
  await section.scrollIntoViewIfNeeded();
  await shot(page, testInfo, "faction-card-unlocked");
  await page.keyboard.press("Escape");
  await page.getByTestId("faction-info-button").nth(1).click();
  await expect(page.getByTestId("faction-breakthrough")).toContainText("locked");
  await page.getByTestId("faction-breakthrough").scrollIntoViewIfNeeded();
  await shot(page, testInfo, "faction-card-locked");
});

test("player sheet on a phone", async ({ page }, testInfo) => {
  await page.setViewportSize({ width: 390, height: 844 });
  const { list, state } = players();
  await openMockedGame(page, { board, players: list, state });
  await page.getByTestId("player-sheet-toggle").click();
  await expect(page.getByTestId("player-breakthrough").first()).toBeVisible();
  await shot(page, testInfo, "sheet-390x844");
});
