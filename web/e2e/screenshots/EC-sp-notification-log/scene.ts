import { expect, type Page } from "@playwright/test";
import { clickOnce, storedAnswers } from "../../offlineDriver";

/** Which side of the change a run is: `before` is taken on the base commit (no asserts about the new UI). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";
export const checking = prefix !== "before";

export const VIEWPORTS = [
  { name: "desktop", width: 1440, height: 900 },
  { name: "phone", width: 390, height: 844 },
] as const;
export type Viewport = (typeof VIEWPORTS)[number];

/** The same table every run: the dev build's seed field makes the bots and the dice repeatable. */
export const SEED = "424242";

/** Starts a seeded 4-player offline game at the given viewport and waits for the board. */
export async function startSeeded(page: Page, viewport: { width: number; height: number }): Promise<string> {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await page.goto("/offline");
  await page.getByLabel("Nickname").fill("Tester");
  await page.getByLabel("Players").selectOption("4");
  await page.getByTestId("offline-seed").fill(SEED);
  await page.getByRole("button", { name: /Start offline game/ }).click();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  const match = /\/offline\/games\/([^/?#]+)/.exec(page.url());
  if (!match) throw new Error(`not on a game page: ${page.url()}`);
  return decodeURIComponent(match[1]);
}

/**
 * Plays (the offline driver's dumb clicks) until the save holds `answers` answers, or `until`
 * says the picture is right. The clicks go through the DOM so a toast over a button cannot stop them.
 */
export async function playTo(page: Page, gameId: string, answers: number, until?: () => Promise<boolean>) {
  let stalled = 0;
  let last = await storedAnswers(page, gameId);
  while (last < answers) {
    if (until && (await until())) return last;
    const clicked = await clickOnce(page);
    await page.waitForTimeout(clicked ? 120 : 300);
    const now = await storedAnswers(page, gameId);
    stalled = now > last ? 0 : stalled + 1;
    expect(stalled, `the game stopped advancing at ${last} answers`).toBeLessThan(80);
    last = now;
  }
  return last;
}

export const toastCount = (page: Page) => page.getByTestId("corner-toast").count();

/** The button that opens the notification log: the new icon in the objectives row, or the old corner pill. */
export const logToggle = (page: Page, viewport: Viewport) =>
  page.getByTestId(!checking || viewport.name === "desktop" ? "toast-history-toggle" : "toast-history-mobile-toggle");

/** Plays on until the action phase shows its bar and `min` toasts are on screen (or `cap` answers). */
export async function reachToasts(page: Page, gameId: string, min: number, cap = 90) {
  const ready = async () => (await page.getByTestId("turn-action-bar").count()) > 0 && (await toastCount(page)) >= min;
  await playTo(page, gameId, cap, ready);
}

/** Controls of the turn bar that something else sits on top of (what the nightly sweep calls an overlap). */
export async function coveredTurnBarControls(page: Page): Promise<string[]> {
  return page.evaluate(() => {
    const out: string[] = [];
    for (const el of Array.from(document.querySelectorAll('[data-testid="turn-action-bar"] button'))) {
      const r = el.getBoundingClientRect();
      if (r.width === 0 || r.height === 0) continue;
      const x = r.left + r.width / 2;
      const y = r.top + r.height / 2;
      if (x < 0 || y < 0 || x >= innerWidth || y >= innerHeight) continue;
      const top = document.elementFromPoint(x, y);
      if (top && !el.contains(top) && !top.contains(el)) out.push(`${el.getAttribute("data-testid")} covered by ${top.getAttribute("data-testid") ?? top.className}`);
    }
    return out;
  });
}
