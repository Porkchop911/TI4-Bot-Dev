import type { Page } from "@playwright/test";
import { openMockedGame, type MockGameOptions } from "../_shared/mockGame";

/** The bottom-left part of the window, where the toasts sit, with the bars next to them. */
export const cornerCrop = (page: Page) => ({ of: page.getByTestId("corner-toasts"), pad: 28 });

import { PROTOCOL_VERSION, type GameEvent } from "../../../src/protocol/types";
import { GAME_ID, type MockedGame } from "../_shared/mockGame";
import { opponent, withPlayer } from "../_shared/players";

/** A third seat for captures that need several other players. */
export const thirdPlayer = withPlayer(opponent, { id: "third_seat", faction: "xxcha" });

/** A public "decision resolved" log entry: `detail` reads "<seat id> <what happened>". */
export function publicEntry(id: string, seat: string, what: string, extra: Partial<GameEvent> = {}): GameEvent {
  return {
    id,
    timestamp: "10:05",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    round: 2,
    phase: "action",
    actor: seat,
    detail: `${seat} ${what}`,
    ...extra,
  } as GameEvent;
}

/** Delivers a new log entry to the open game, as the server does when something happens. */
export function pushEntry(game: MockedGame, entry: GameEvent) {
  game.send({ type: "event", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, entry });
}

/**
 * Opens the mocked game and waits until the board shows. Log entries that are already there when
 * the game loads are history, so captures push theirs only after this.
 */
export async function openCornerGame(page: Page, options: MockGameOptions) {
  const game = await openMockedGame(page, options);
  await page.getByTestId("ti4-board-svg").waitFor();
  return game;
}
