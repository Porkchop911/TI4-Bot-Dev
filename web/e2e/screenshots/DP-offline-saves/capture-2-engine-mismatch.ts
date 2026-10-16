import { expect, test } from "@playwright/test";
import { playUntil } from "../../offlineDriver";
import { shot } from "../_shared/shot";
import { ENGINE_LABEL, SIZES, blockServer, foldDecision, newGame, pretendOtherEngine } from "./offlineSaves";

// A game saved with another build of the engine: the page asks before it replays anything.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`engine mismatch ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(180_000);
    await page.setViewportSize(viewport);
    await blockServer(page);
    const id = await newGame(page, { players: 4, seat: 3, seed: 11 });
    await playUntil(page, id, 8);
    await page.waitForTimeout(400);
    await pretendOtherEngine(page, id);

    await page.goto("/offline");
    await page.getByTestId("offline-resume").click();
    const dialog = page.getByTestId("offline-mismatch");
    await expect(dialog).toBeVisible({ timeout: 60_000 });
    await expect(dialog).toHaveAttribute("data-reason", "engine-changed");
    await shot(page, testInfo, `1-engine-changed-${size}`);

    // Delete asks once more.
    await page.getByTestId("offline-mismatch-delete").click();
    await expect(page.getByTestId("offline-mismatch-delete-confirm")).toBeVisible();
    await shot(page, testInfo, `2-delete-confirm-${size}`);
    await page.getByRole("button", { name: "Keep" }).click();

    // View only: the game opens, moves are refused, the strip says so and names the engine.
    await page.getByTestId("offline-mismatch-view-only").click();
    await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 120_000 });
    await expect(page.getByTestId("offline-view-only")).toBeVisible();
    await expect(page.getByTestId("offline-engine")).toHaveText(ENGINE_LABEL);
    await foldDecision(page);
    await shot(page, testInfo, `3-view-only-${size}`);
  });
}
