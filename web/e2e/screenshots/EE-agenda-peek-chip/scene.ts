import { expect, type Page, type TestInfo } from "@playwright/test";
import { printed } from "../EA-action-card-pickers/scene";
import { galleryBoard } from "../_shared/fixtures";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import { shot } from "../_shared/shot";

/** Which side of the change a run is: `before` is taken on the commit with the 60dvh fixed card. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const VIEWPORTS = [
  { id: "phone", width: 390, height: 844 },
  { id: "desktop", width: 1440, height: 900 },
] as const;
export type Viewport = (typeof VIEWPORTS)[number];

const peekCards = () =>
  ["minister_commerce", "mutiny", "secret"].map((alias) => {
    const card = printed(alias);
    return { id: card.id, name: card.name, type: card.type, target: card.target, text1: card.text1, text2: card.text2 };
  });

/** The viewer's own turn menu in the action phase, holding what Insider Information showed. */
export async function openTurnWithPeek(page: Page, viewport: Viewport) {
  await page.setViewportSize({ width: viewport.width, height: viewport.height });
  await openMockedGame(page, {
    board: galleryBoard,
    players: [playerWithHand({ agenda_peek: peekCards() }), opponent],
    choice: {
      prompt: "action phase",
      options: [
        { id: "tactical", label: "take a tactical action", kind: "action" },
        { id: "strategic|pok3politics", label: "take the strategic action of 3. Politics", kind: "action" },
        { id: "pass", label: "pass", kind: "action" },
      ],
      details: {
        kind: "turn_menu",
        closing: false,
        tokens: { tactic: 3, fleet: 3, strategy: 2 },
        strategy_cards: [{ card: "pok3politics", used: false, option: "strategic|pok3politics" }],
        partners: [],
      },
    },
  });
  await page.getByTestId("turn-action-bar").waitFor();
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.getByTestId("agenda-peek").waitFor();
  await page.waitForTimeout(600);
}

/** Every button of the turn bar must be the topmost element at its centre (not covered by the peek). */
export async function assertTurnBarReachable(page: Page) {
  const covered = await page.evaluate(() => {
    const bar = document.querySelector<HTMLElement>('[data-testid="turn-action-bar"]')!;
    return Array.from(bar.querySelectorAll<HTMLElement>("button"))
      .filter((b) => b.getBoundingClientRect().height > 0)
      .filter((b) => {
        const r = b.getBoundingClientRect();
        const top = document.elementFromPoint(r.left + r.width / 2, r.top + r.height / 2);
        return !top || !b.contains(top);
      })
      .map((b) => (b.textContent ?? "").trim());
  });
  expect(covered, "turn bar buttons covered by something else").toEqual([]);
}

export const takeShot = (page: Page, testInfo: TestInfo, name: string, viewport: Viewport) =>
  shot(page, testInfo, `${prefix}-${name}-${viewport.id}`);
