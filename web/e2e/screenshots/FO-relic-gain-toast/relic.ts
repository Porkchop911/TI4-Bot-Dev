import { expect, type Page } from "@playwright/test";
import { openMockedGame, type MockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";
import { galleryBoard } from "../_shared/fixtures";

export const SIZES = { desktop: { width: 1440, height: 900 }, "390x844": { width: 390, height: 844 } } as const;
export { actor, opponent };

/** The engine's public relic log, as the state carries it. */
export const relicLog = (records: Record<string, unknown>[]) => ({ relic_log: records });

/** Opens the app on a quiet game, then lets a state update add `records` to the relic log. */
export async function gainRelics(
  page: Page,
  viewport: { width: number; height: number },
  records: Record<string, unknown>[],
): Promise<MockedGame> {
  await page.setViewportSize(viewport);
  const game = await openMockedGame(page, {
    board: galleryBoard,
    players: [playerWithHand({}), opponent],
    phase: "action",
    round: 3,
    state: relicLog([]),
  });
  await page.waitForTimeout(500);
  const { events: _events, ...update } = game.snapshot;
  game.send({ ...update, type: "state_update", game_version: 41, state: relicLog(records) });
  await page.locator('[data-testid="corner-toast"][data-toast-kind="relic"]').first().waitFor();
  await page.waitForTimeout(700); // the toast slides in
  // The same log in a later state does not toast twice.
  game.send({ ...update, type: "state_update", game_version: 42, state: relicLog(records) });
  await page.waitForTimeout(300);
  await expect(page.locator('[data-toast-kind="relic"]')).toHaveCount(records.length);
  return game;
}

export const toasts = (page: Page) => page.getByTestId("corner-toasts");
