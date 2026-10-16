import { expect, test } from "@playwright/test";
import { shot } from "../_shared/shot";
import { GAMES, SIZES, openOffline } from "./offline";

for (const [size, viewport] of Object.entries(SIZES)) {
  test(`my offline games ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openOffline(page, "/offline", { games: GAMES });
    await expect(page.getByTestId("offline-game-row")).toHaveCount(3);
    await page.getByTestId("offline-games").scrollIntoViewIfNeeded();
    await page.evaluate(() => window.scrollTo(0, document.body.scrollHeight));
    await shot(page, testInfo, `3-my-offline-games-${size}`);
    await page.getByTestId("offline-delete").first().click();
    await expect(page.getByTestId("offline-delete-confirm")).toBeVisible();
    await shot(page, testInfo, `4-delete-confirm-${size}`);
    await page.getByTestId("offline-delete-confirm").click();
    await expect(page.getByTestId("offline-game-row")).toHaveCount(2);
  });
}

for (const [size, viewport] of Object.entries(SIZES)) {
  test(`resume progress ${size}`, async ({ page }, testInfo) => {
    await page.setViewportSize(viewport);
    await openOffline(page, "/offline", { games: GAMES, replaySteps: 612, freezeAt: 0.45 });
    await page.getByTestId("offline-resume").first().click();
    await expect(page.getByTestId("offline-progress-text")).toContainText("of 612 (45%)");
    expect(await page.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("45");
    await shot(page, testInfo, `5-resume-progress-${size}`);
  });
}

test("loading the engine is indeterminate", async ({ page }, testInfo) => {
  await openOffline(page, "/offline", { games: GAMES, replayMs: 60_000 });
  await page.getByTestId("offline-start").waitFor();
  await page.getByTestId("offline-nickname").fill("Ada");
  await page.getByTestId("offline-start").click();
  await expect(page.getByTestId("offline-progress")).toBeVisible();
  expect(await page.getByRole("progressbar").getAttribute("aria-valuenow")).toBeNull();
  await shot(page, testInfo, "6-engine-loading");
});

test("browser without JSPI", async ({ page }, testInfo) => {
  await openOffline(page, "/offline", { games: GAMES, unsupported: "Offline play needs Chrome or Edge 137 or newer." });
  await expect(page.getByTestId("offline-unsupported")).toBeVisible();
  await expect(page.getByTestId("offline-setup-form")).toHaveCount(0);
  await shot(page, testInfo, "7-unsupported-browser");
});

test("a damaged save", async ({ page }, testInfo) => {
  await openOffline(page, "/offline", { games: GAMES, replayMs: 100, failResume: "This save could not be replayed (engine version changed). Export it and report it." });
  await page.getByTestId("offline-resume").first().click();
  await expect(page.getByTestId("offline-error")).toBeVisible();
  await shot(page, testInfo, "8-resume-failed");
});
