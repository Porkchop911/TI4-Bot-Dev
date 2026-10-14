import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAMES, SIZES, openOffline } from "./offline";

for (const [size, viewport] of Object.entries(SIZES)) {
  test(`start screen entry and setup form ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openOffline(page, "/", { games: GAMES });
    const entry = page.getByTestId("play-offline-button");
    await expect(entry).toBeVisible();
    await shot(page, testInfo, `1-start-screen-${size}`);
    await entry.click();
    await expect(page.getByTestId("offline-setup-form")).toBeVisible();
    await page.getByTestId("offline-nickname").fill("Ada");
    await page.getByTestId("offline-player-count").selectOption("5");
    await page.getByTestId("offline-seat").selectOption("3");
    await expect(page.getByTestId("offline-banner")).toContainText("Not available offline");
    await shot(page, testInfo, `2-setup-form-${size}`);
    if (size === "390x844") {
      const overflow = await page.evaluate(() => document.documentElement.scrollWidth > window.innerWidth);
      expect(overflow).toBe(false);
    }
  });
}

test("no entry without a launcher, and an invalid seed is reported", async ({ page }) => {
  await page.route("**/api/**", (route) => route.fulfill({ status: 503, json: {} }));
  await page.goto("/");
  await page.getByTestId("create-game-button").waitFor();
  // Only a build without the engine file has no launcher (a build with it registers the real one).
  if (!existsSync(resolve(process.cwd(), "src/offline/local/engine/ti4_wasm.wasm")))
    await expect(page.getByTestId("play-offline-button")).toHaveCount(0);
  await openOffline(page, "/offline");
  await page.getByTestId("offline-nickname").fill("Ada");
  await page.getByTestId("offline-seed").fill("-4");
  await page.getByTestId("offline-start").click();
  await expect(page.getByTestId("offline-setup-error")).toContainText("Seed");
});
