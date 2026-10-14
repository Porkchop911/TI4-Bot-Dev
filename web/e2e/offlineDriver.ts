import type { Page } from "@playwright/test";
import { tmpdir } from "node:os";
import { join } from "node:path";

/**
 * Helpers that play an offline game through the real UI (the production build, no server).
 * The decision clicks are deliberately dumb: confirm what is selected, else take the first
 * action-phase move, else pass.
 */

export const NICKNAME = "Tester";

/** Starts a game from the setup form and waits for the board. */
export async function startOfflineGame(page: Page, players = 4): Promise<string> {
  await page.goto("/offline");
  await page.getByLabel("Nickname").fill(NICKNAME);
  await page.getByLabel("Players").selectOption(String(players));
  await page.getByRole("button", { name: /Start offline game/ }).click();
  await page.waitForSelector('[data-testid="offline-game"]', { timeout: 60_000 });
  const match = /\/offline\/games\/([^/?#]+)/.exec(page.url());
  if (!match) throw new Error(`not on a game page: ${page.url()}`);
  return decodeURIComponent(match[1]);
}

/** What the browser's IndexedDB holds for a game (the save store, `ti4-offline`); null if nothing yet. */
export interface StoredRecord {
  answers: number;
  decisionsHash: string;
  round: number | null;
  phase: string | null;
  wasmSha256: string;
}

export async function storedRecord(page: Page, gameId: string): Promise<StoredRecord | null> {
  return page.evaluate(
    (id) =>
      new Promise<StoredRecord | null>((resolve) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => resolve(null);
        open.onsuccess = () => {
          const db = open.result;
          try {
            const get = db.transaction("games").objectStore("games").get(id);
            get.onsuccess = () => {
              db.close();
              const r = get.result;
              resolve(
                r
                  ? {
                      answers: r.save.answers.length,
                      decisionsHash: r.save.decisions_hash,
                      round: r.save.round ?? null,
                      phase: r.save.phase ?? null,
                      wasmSha256: r.engine.wasmSha256,
                    }
                  : null,
              );
            };
            get.onerror = () => {
              db.close();
              resolve(null);
            };
          } catch {
            db.close();
            resolve(null);
          }
        };
      }),
    gameId,
  );
}

/** Number of the human's answers in the saved game. */
export async function storedAnswers(page: Page, gameId: string): Promise<number> {
  return (await storedRecord(page, gameId))?.answers ?? 0;
}

/** The feedback queue as stored in IndexedDB: `[state, reportId]` of each item. */
export async function storedFeedback(page: Page): Promise<Array<{ state: string; reportId: string; attempts: number }>> {
  return page.evaluate(
    () =>
      new Promise((resolve) => {
        const open = indexedDB.open("ti4-offline");
        open.onerror = () => resolve([]);
        open.onsuccess = () => {
          const db = open.result;
          const all = db.transaction("feedback").objectStore("feedback").getAll();
          all.onsuccess = () => {
            db.close();
            resolve(all.result.map((i: { state: string; reportId: string; attempts: number }) => ({ state: i.state, reportId: i.reportId, attempts: i.attempts })));
          };
          all.onerror = () => {
            db.close();
            resolve([]);
          };
        };
      }),
  );
}

const CLICKS = [
  '[data-testid="submit-choice-button"]:not([disabled]):not([aria-disabled="true"])',
  '[data-testid^="turn-bar-strategic-"]:not([disabled]):not([aria-disabled="true"])',
  // A reaction window: pass on it.
  'button:text-matches("^Pass")',
  '[data-testid="turn-bar-pass"]:not([disabled]):not([aria-disabled="true"])',
  '[data-testid="turn-bar-end"]:not([disabled]):not([aria-disabled="true"])',
  // Optional steps (a structure, an ability, a secondary) and bot trade offers: decline.
  'button:text-matches("^(Skip|Decline|refuse)$")',
  // Production: build nothing further.
  '[data-testid="done-producing-btn"]:not([disabled]):not([aria-disabled="true"])',
  // Anything else that asks for a confirmation.
  '[data-testid="pending-choice-dialog"] button.button--primary:not([disabled]):not([aria-disabled="true"])',
];

/** Leadership and friends: assign the tokens to the first pool, then confirm. */
async function assignTokens(page: Page): Promise<boolean> {
  // (role queries are slow on the big map DOM: use plain selectors)
  const confirm = page.locator('button:text-matches("Confirm tokens")');
  if ((await confirm.count()) === 0) return false;
  if (await confirm.isEnabled()) {
    await confirm.click({ timeout: 2_000 }).catch(() => undefined);
    return true;
  }
  await page.locator('[data-testid^="token-plus-"]:not([disabled]):not([aria-disabled="true"])').first().click({ timeout: 2_000 }).catch(() => undefined);
  return true;
}

/** One click at the first control that moves the game; false when none is on screen. */
export async function clickOnce(page: Page): Promise<boolean> {
  if (await assignTokens(page)) return true;
  for (const selector of CLICKS) {
    const target = page.locator(selector).first();
    if ((await target.count()) === 0) continue;
    try {
      await target.click({ timeout: 1_000 });
      return true;
    } catch {
      // covered or gone: try the next control
    }
  }
  return false;
}

/** Plays until the saved game holds `answers` answers; throws if the game stops advancing. */
export async function playUntil(page: Page, gameId: string, answers: number, log: (line: string) => void = () => {}) {
  let last = await storedAnswers(page, gameId);
  let stalled = 0;
  while (last < answers) {
    const clicked = await clickOnce(page);
    await page.waitForTimeout(clicked ? 150 : 400);
    const now = await storedAnswers(page, gameId);
    if (now > last) stalled = 0;
    else if (++stalled > 60) {
      const picture = join(tmpdir(), "ti4-offline-stalled.png");
      await page.screenshot({ path: picture }).catch(() => undefined);
      throw new Error(`the game stopped advancing at ${last} answers (screenshot: ${picture})`);
    }
    if (now !== last) log(`answers ${now}`);
    last = now;
  }
  return last;
}
