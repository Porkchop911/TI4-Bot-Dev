import { expect, type Page } from "@playwright/test";
import { openMockedGame, type MockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { openCornerGame, publicEntry, pushEntry, thirdPlayer } from "../G-corner-notifications/corner";

export const SIZES = {
  "360x740": { width: 360, height: 740 },
  "390x844": { width: 390, height: 844 },
  "412x915": { width: 412, height: 915 },
  desktop: { width: 1440, height: 900 },
} as const;
export type SizeName = keyof typeof SIZES;
export const PHONES: SizeName[] = ["360x740", "390x844", "412x915"];

export const DOCK_IDS = ["player-sheet-toggle", "objectives-mobile-button", "toast-history-mobile-toggle", "event-log-mobile-toggle", "feedback-button"];

const nicknames = { other_seat: "Mira", third_seat: "Tobi", gallery_seat: "You" };

/**
 * A multiplayer game (mocked server, a real seat) on the public server, with several toasts
 * pushed after load: other seats' actions, so the bell shows an unread badge.
 */
export async function openScene(page: Page, size: SizeName, options: { spectator?: boolean; toasts?: boolean } = {}): Promise<MockedGame> {
  await page.setViewportSize(SIZES[size]);
  await page.route("**/health", (route) =>
    route.fulfill({ json: { status: "ok", protocol_version: 3, commit: "51390594", public: true } }),
  );
  await page.route("**/api/access", (route) => route.fulfill({ json: { required: true, granted: true } }));
  const players = [playerWithHand({ technologies: ["amd", "st"] }), opponent, thirdPlayer];
  const game = options.spectator
    ? await openMockedGame(page, { players, spectator: true, nicknames })
    : await openCornerGame(page, {
        players,
        nicknames,
        table: { revealed_objectives: ["amass_wealth", "develop", "corner"], scored_objectives: { gallery_seat: ["amass_wealth"] } },
        choice: {
          prompt: "action phase",
          options: [
            { id: "tactical", label: "take a tactical action", kind: "action" },
            { id: "pass", label: "pass", kind: "action" },
          ],
          details: { kind: "turn_menu", closing: false, tokens: { tactic: 3, fleet: 3, strategy: 2 }, strategy_cards: [], partners: [] },
        },
      });
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(300);
  if (options.toasts !== false) {
    pushEntry(game, publicEntry("t1", "other_seat", "played Direct Hit"));
    pushEntry(game, publicEntry("t2", "third_seat", "played Sabotage"));
    pushEntry(game, publicEntry("t3", "other_seat", "activated #22"));
    pushEntry(game, publicEntry("t4", "third_seat", "moved carrier from #19 to #22"));
    await page.locator('[data-testid="toast-history-unread"]:visible').first().waitFor();
    await expect(page.getByTestId("corner-toast").first()).toBeVisible();
    await page.waitForTimeout(400);
  }
  return game;
}

/** Whether a tap at the element's centre would hit it (not covered by something else). */
export async function tappable(page: Page, testId: string): Promise<boolean> {
  return page.getByTestId(testId).evaluate((el) => {
    const r = el.getBoundingClientRect();
    const hit = document.elementFromPoint(r.x + r.width / 2, r.y + r.height / 2);
    return hit === el || el.contains(hit);
  });
}

/** The dock's buttons in one row, each at least 44 px, none overlapping, inside the window. */
export async function dockRects(page: Page) {
  return page
    .locator(".app-shell__mobile-actions > *")
    .evaluateAll((els) => els.map((e) => e.getBoundingClientRect()).map((r) => ({ l: r.left, r: r.right, t: r.top, w: r.width, h: r.height })));
}
