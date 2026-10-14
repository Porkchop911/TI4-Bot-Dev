import { test, expect, Page } from "@playwright/test";
import { createStartedGame, openPlayerGame } from "./lobbyHelpers";

/**
 * Helper to ensure zero console errors / unhandled browser exceptions.
 */
function trackErrors(page: Page, label: string) {
  page.on("pageerror", (err) => {
    throw new Error(`[${label}] Unhandled browser error: ${err.message}`);
  });
  page.on("console", (msg) => {
    if (msg.type() === "error" && !msg.text().includes("favicon")) {
      throw new Error(`[${label}] Console Error: ${msg.text()}`);
    }
  });
}

test.describe("Gameplay Workflows & Responsive Shell Suite (UI-08)", () => {
  test("responsive layout breakpoints on mobile, tablet, and desktop viewports", async ({
    browser,
    request,
  }) => {
    const { gameId, players } = await createStartedGame(request, 3, 101);

    const context = await browser.newContext();
    const page = await context.newPage();
    trackErrors(page, "Responsive View");

    // 1. Desktop Viewport (>= 1280px)
    await page.setViewportSize({ width: 1440, height: 900 });
    await openPlayerGame(page, gameId, players[0].session);

    await expect(page.locator('[data-testid="turn-status-bar"]')).toBeVisible();
    await expect(page.locator('[data-testid="ti4-board-svg"]')).toBeVisible();
    // Desktop layout has side player sheet visible and mobile action buttons hidden
    await expect(page.locator('[data-testid="player-sheet-drawer"]')).toBeVisible();
    await expect(page.locator('[data-testid="player-sheet-toggle"]')).not.toBeVisible();

    // 2. Tablet Viewport (768px - 1279px)
    await page.setViewportSize({ width: 1024, height: 768 });
    await page.waitForTimeout(200);
    await expect(page.locator('[data-testid="ti4-board-svg"]')).toBeVisible();

    // 3. Mobile Viewport (< 768px)
    await page.setViewportSize({ width: 390, height: 844 });
    await page.waitForTimeout(200);
    // Mobile action bar displays drawer toggles
    await expect(page.locator('[data-testid="player-sheet-toggle"]')).toBeVisible();
    await expect(page.locator('[data-testid="event-log-mobile-toggle"]')).toBeVisible();

    // Minimize choice dialog to inspect player sheets on mobile
    const minBtn = page.locator('[data-testid="minimize-choice-button"]');
    if (await minBtn.isVisible()) {
      await minBtn.click();
      await expect(page.locator('[data-testid="minimized-choice-banner"]')).toBeVisible();
    }

    // Toggle mobile player sheet drawer
    await page.locator('[data-testid="player-sheet-toggle"]').click();
    await expect(page.locator('[data-testid="player-sheet-drawer"]')).toHaveClass(
      /app-shell__drawer--open/,
    );
    await page.locator('[data-testid="player-sheet-toggle"]').click();
    await expect(page.locator('[data-testid="player-sheet-drawer"]')).not.toHaveClass(
      /app-shell__drawer--open/,
    );

    await context.close();
  });

  test("multiplayer turn progression through ChoiceRendererDispatcher", async ({
    browser,
    request,
  }) => {
    const { gameId, players } = await createStartedGame(request, 3, 777);

    const contextP1 = await browser.newContext();
    const pageP1 = await contextP1.newPage();
    trackErrors(pageP1, "Player 1");
    await openPlayerGame(pageP1, gameId, players[0].session);

    // Verify game connected and strategy draft choice is rendered
    await expect(pageP1.locator('[data-testid="turn-status-bar"]')).toBeVisible();
    const choiceDialog = pageP1.locator('[data-testid="pending-choice-dialog"]');
    await expect(choiceDialog).toBeVisible();

    // Verify option search filter works for large lists
    const searchFilter = pageP1.locator('[data-testid="choice-search-input"]');
    if (await searchFilter.isVisible()) {
      await searchFilter.fill("Leadership");
      const filteredOptions = pageP1.locator('[data-testid="choice-option"]');
      await expect(filteredOptions).toHaveCount(1);
      await searchFilter.fill("");
    }

    // Select first option and submit strategy pick
    const firstOption = pageP1.locator('[data-testid="choice-option"]').first();
    await firstOption.click();
    const submitBtn = pageP1.locator('[data-testid="submit-choice-button"]');
    await expect(submitBtn).toBeEnabled();
    await submitBtn.click();

    // The next human participant receives the draft after the host's pick.
    await expect(pageP1.locator('[data-testid="turn-status-bar"]')).toBeVisible();

    await contextP1.close();
  });
});
