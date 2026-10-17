import { expect, test, type Page } from "@playwright/test";
import { clickOnce, storedAnswers } from "../../offlineDriver";
import { shot } from "../_shared/shot";

// The "Undo my last turn" button in a single-player (offline) game, real engine game with a fixed
// seed. Plays until the person has taken a few turns, opens the event log, shows the button, undoes
// the last turn and asserts that the game is back at the start of that turn (fewer answers saved,
// the action-phase menu asked again), that it plays on afterwards and that a reload resumes there.

const VIEWPORTS = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "phone", width: 390, height: 844 },
] as const;
const SEED = "424242";

async function start(page: Page, viewport: { width: number; height: number }): Promise<string> {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await page.goto("/offline");
  await page.getByLabel("Nickname").fill("Tester");
  await page.getByLabel("Players").selectOption("4");
  await page.getByTestId("offline-seed").fill(SEED);
  await page.getByRole("button", { name: /Start offline game/ }).click();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  return decodeURIComponent(/\/offline\/games\/([^/?#]+)/.exec(page.url())![1]);
}

/** Plays until the person has answered `answers` times with the turn bar on screen at the end. */
async function playTo(page: Page, gameId: string, answers: number) {
  let last = await storedAnswers(page, gameId);
  let stalled = 0;
  while (last < answers || (await page.getByTestId("turn-action-bar").count()) === 0) {
    const clicked = await clickOnce(page);
    await page.waitForTimeout(clicked ? 120 : 300);
    const now = await storedAnswers(page, gameId);
    stalled = now > last ? 0 : stalled + 1;
    expect(stalled, `the game stopped advancing at ${last} answers`).toBeLessThan(80);
    last = now;
  }
  return last;
}

async function openLog(page: Page, name: string) {
  await page.getByTestId(name === "desktop" ? "event-log-toggle" : "event-log-mobile-toggle").dispatchEvent("click");
  await expect(page.getByTestId("event-log-list")).toBeVisible();
}

/** The toasts are not what these pictures are about: hide the stack so it does not cover the log. */
async function hideToasts(page: Page) {
  await page.addStyleTag({ content: '[data-testid="corner-toasts"]{display:none!important}' });
}

async function foldDialog(page: Page) {
  const minimize = page.getByTestId("minimize-choice-button");
  if (await minimize.count()) await minimize.first().dispatchEvent("click");
  await page.waitForTimeout(300);
}

for (const viewport of VIEWPORTS) {
  test(`undo my last turn at ${viewport.name}`, async ({ page }, testInfo) => {
    test.setTimeout(240_000);
    const gameId = await start(page, viewport);
    const answers = await playTo(page, gameId, 30);
    await page.waitForTimeout(600);
    await foldDialog(page);
    await hideToasts(page);
    await openLog(page, viewport.name);

    const button = page.getByTestId("turn-redo-btn");
    await expect(button).toBeVisible();
    await expect(button).toHaveText("Undo my last turn");
    await expect(button).toBeEnabled();
    await expect(page.getByTestId("turn-redo-seat")).toHaveCount(0);
    await shot(page, testInfo, `after-button-${viewport.name}`, { of: page.getByTestId("event-log-turn-redo"), pad: 24 });
    await shot(page, testInfo, `after-log-${viewport.name}`);

    await button.dispatchEvent("click");
    await expect.poll(() => storedAnswers(page, gameId), { timeout: 60_000 }).toBeLessThan(answers);
    const undone = await storedAnswers(page, gameId);
    expect(undone).toBeGreaterThan(0);
    // The person is asked the start of that turn again.
    await expect(page.getByTestId("turn-action-bar")).toBeVisible({ timeout: 60_000 });
    await foldDialog(page);
    await shot(page, testInfo, `after-undone-${viewport.name}`);

    // A reload resumes at the undone position, and the game plays on.
    await page.reload();
    await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
    expect(await storedAnswers(page, gameId)).toBe(undone);
    await expect(page.getByTestId("turn-action-bar")).toBeVisible({ timeout: 60_000 });
    for (let i = 0; i < 20; i += 1) {
      await clickOnce(page);
      await page.waitForTimeout(200);
    }
    expect(await storedAnswers(page, gameId)).toBeGreaterThan(undone);
  });
}
