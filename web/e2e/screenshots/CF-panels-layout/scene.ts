import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";

/** `before` is taken on the commit without the changes; its assertions are skipped. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const tokens = { tactic: 3, fleet: 3, strategy: 2 };

/** Six seats, so the right-hand column is taller than the screen. */
export const sixPlayers = () => [
  playerWithHand({}),
  opponent,
  ...[2, 3, 4, 5].map((i) => ({ ...opponent, id: `seat${i}` })),
];

export const turnMenu = {
  prompt: "action phase",
  options: [
    { id: "tactical", label: "take a tactical action", kind: "action" },
    { id: "pass", label: "pass", kind: "action" },
  ],
  details: { kind: "turn_menu", closing: false, tokens, strategy_cards: [], partners: [] },
};

export async function openTurn(page: Page, size: { width: number; height: number }) {
  await page.setViewportSize(size);
  const game = await openMockedGame(page, { players: sixPlayers(), choice: turnMenu });
  await page.getByTestId("turn-action-bar").waitFor();
  return game;
}
