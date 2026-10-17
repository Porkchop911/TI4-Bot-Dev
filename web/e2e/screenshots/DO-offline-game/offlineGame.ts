import { expect, type Page } from "@playwright/test";
import { existsSync } from "node:fs";
import { resolve } from "node:path";
import { clickOnce, storedAnswers } from "../../offlineDriver";

export const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;

/** The seed of every capture: the engine and the rule bots are deterministic, so is the table. */
export const SEED = 11;

export function requireEngine() {
  const engine = resolve(process.cwd(), "src/offline/local/engine/ti4_wasm.wasm");
  if (!existsSync(engine))
    throw new Error("no engine file: run `npm run build:wasm` in web/ first (see e2e/screenshots/README.md)");
}

/**
 * Opens the setup form of the real app (real launcher, real engine in a Web Worker) with every
 * call to a server blocked, and starts a game: 4 players, seat 3, seeded.
 */
export async function startGame(page: Page): Promise<string> {
  requireEngine();
  await page.route(/\/(api|ws|health)(\/|$)/, (route) => route.abort());
  await page.goto("/offline");
  await page.getByTestId("offline-nickname").fill("Ada");
  await page.getByTestId("offline-player-count").selectOption("4");
  await page.getByTestId("offline-seat").selectOption("3");
  await page.getByTestId("offline-seed").fill(String(SEED));
  await page.getByTestId("offline-start").click();
  await expect(page.getByTestId("offline-game")).toBeVisible({ timeout: 60_000 });
  const id = /\/offline\/games\/([^/?#]+)/.exec(page.url())?.[1];
  if (!id) throw new Error(`not on a game page: ${page.url()}`);
  return decodeURIComponent(id);
}

/**
 * Plays through the real UI until it is the human's turn to act (the turn bar with its Pass
 * button, no decision dialog open), i.e. the start of the action phase.
 */
export async function playToActionPhase(page: Page, id: string, maxClicks = 400) {
  for (let i = 0; i < maxClicks; i++) {
    const turnBar = await page.getByTestId("turn-bar-pass").count();
    const dialog = await page.locator('[data-testid="pending-choice-dialog"], [data-testid="decision-modal"]').count();
    if (turnBar > 0 && dialog === 0) return;
    const clicked = await clickOnce(page);
    await page.waitForTimeout(clicked ? 120 : 300);
  }
  throw new Error(`never reached the action phase (answers: ${await storedAnswers(page, id)})`);
}
