import { expect, test } from "@playwright/test";
import { mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { shot } from "../_shared/shot";
import { SIZES, blockServer, showGamesList, threeGames } from "./offlineSaves";

// "My offline games" with several real games (three tables of 3, 4 and 5 players, saved in
// IndexedDB), the Import a game button, and what importing does with a good and a bad file.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`saved games, import and export ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(240_000);
    await page.setViewportSize(viewport);
    await blockServer(page);
    const ids = await threeGames(page);
    await page.goto("/offline");
    await expect(page.getByTestId("offline-game-row")).toHaveCount(3);
    await expect(page.getByTestId("offline-import")).toBeVisible();
    await showGamesList(page);
    await shot(page, testInfo, `1-saved-games-${size}`);

    // Export one game to a file, delete it, bring it back with Import.
    const [download] = await Promise.all([page.waitForEvent("download"), page.getByTestId("offline-export").nth(2).click()]);
    const file = join(mkdtempSync(join(tmpdir(), "ti4-dp-")), download.suggestedFilename());
    await download.saveAs(file);
    await page.getByTestId("offline-delete").nth(2).click();
    await page.getByTestId("offline-delete-confirm").click();
    await expect(page.getByTestId("offline-game-row")).toHaveCount(2);
    await page.getByTestId("import-game-file").setInputFiles(file);
    await expect(page.getByTestId("offline-notice")).toHaveText("Game imported.");
    await expect(page.getByTestId("offline-game-row")).toHaveCount(3);
    await showGamesList(page);
    await shot(page, testInfo, `2-import-done-${size}`);

    // A file that is not an exported game is refused with a reason; nothing is added.
    const bad = join(mkdtempSync(join(tmpdir(), "ti4-dp-")), "notes.json");
    writeFileSync(bad, JSON.stringify({ hello: "world" }));
    await page.getByTestId("import-game-file").setInputFiles(bad);
    await expect(page.getByTestId("offline-error")).toBeVisible();
    await expect(page.getByTestId("offline-game-row")).toHaveCount(3);
    await showGamesList(page);
    await shot(page, testInfo, `3-import-refused-${size}`);
    expect(ids).toHaveLength(3);
  });
}
