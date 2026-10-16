import type { Page } from "@playwright/test";
import { GAME_ID, openMockedGame, type MockGameOptions } from "../_shared/mockGame";
import { playerWithHand, opponent } from "../_shared/players";
import { PROTOCOL_VERSION, type ReactionIntentStateMsg } from "../../../src/protocol/types";

/** The seat's own bluff state as the server sends it to that seat only. */
export const intentState = (over: Partial<ReactionIntentStateMsg> = {}) => ({
  type: "reaction_intent_state" as const,
  protocol_version: PROTOCOL_VERSION,
  game_id: GAME_ID,
  triggers: [],
  max_triggers: 3,
  eligible: true,
  budget_used_up: false,
  holding: false,
  ...over,
});

export const ROUND = 2;

/**
 * The real app against a mocked server that answers `set_reaction_intent` the way the real one
 * does: the whole set is accepted and locked until the next round.
 */
export async function openBluffGame(
  page: Page,
  options: MockGameOptions & { width?: number; height?: number } = {},
) {
  const { width = 1440, height = 900, ...rest } = options;
  await page.setViewportSize({ width, height });
  const game = await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    round: ROUND,
    onClientMessage: (message, reply) => {
      if (message.type === "set_reaction_intent")
        reply(
          intentState({
            triggers: message.triggers,
            locked_until_round: message.triggers.length ? ROUND + 1 : undefined,
          }),
        );
    },
    ...rest,
  });
  await page.getByTestId("player-sheet-panel").waitFor();
  return game;
}

export const ownCard = (page: Page) =>
  page.locator('[data-testid="player-card"][data-is-self="true"]');

/** Opens the "Bluff a reaction" section on the viewer's own card. */
export async function openSelector(page: Page) {
  const selector = page.getByTestId("bluff-selector");
  await selector.waitFor();
  if (!(await selector.evaluate((el) => (el as HTMLDetailsElement).open)))
    await selector.locator("summary").click();
  return selector;
}
