import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { SIZES, openVersions, patchStoredSave, startGame } from "./versions";

// The same game opened again after its save was set to an older bot and older rules: it keeps
// them, and the list says so. A save from before the fields shows the legacy notes.
for (const [size, viewport] of Object.entries(SIZES)) {
  test(`older version ${size}`, async ({ page }, testInfo) => {
    test.setTimeout(120_000);
    await page.setViewportSize(viewport);
    const id = await startGame(page);
    await patchStoredSave(page, id, { bot_version: 3, rules_version: 9 });
    await page.reload();
    await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 60_000 });
    await expect(page.getByTestId("pending-choice-dialog")).toBeVisible({ timeout: 60_000 });
    await openVersions(page, size);
    await expect(page.getByTestId("version-bot")).toContainText("version 3");
    await expect(page.getByTestId("version-bot-note")).toContainText("older bot");
    const rules = page.getByTestId("version-rules");
    await expect(rules).toContainText("version 9");
    await expect(rules).toHaveAttribute("data-older", "true");
    await expect(page.getByTestId("version-rules-note")).toContainText("keeps the older rules");
    await shot(page, testInfo, `2-older-${size}`);
  });
}

test("a save from before the fields desktop", async ({ page }, testInfo) => {
  test.setTimeout(120_000);
  await page.setViewportSize(SIZES.desktop);
  const id = await startGame(page);
  await patchStoredSave(page, id, { bot_version: null, rules_version: null });
  await page.reload();
  await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 60_000 });
  await expect(page.getByTestId("pending-choice-dialog")).toBeVisible({ timeout: 60_000 });
  await openVersions(page, "desktop");
  await expect(page.getByTestId("version-bot")).toContainText("version 1");
  await expect(page.getByTestId("version-bot-note")).toContainText("before bot versions");
  await expect(page.getByTestId("version-rules")).toContainText("legacy");
  await expect(page.getByTestId("version-rules-note")).toContainText("legacy rules");
  await shot(page, testInfo, "3-before-the-fields-desktop");
});
