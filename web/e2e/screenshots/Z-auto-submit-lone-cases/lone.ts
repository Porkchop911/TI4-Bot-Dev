import type { Page } from "@playwright/test";
import { PROTOCOL_VERSION, type ClientMessage } from "../../../src/protocol/types";
import { GAME_ID, openMockedGame, type MockChoice, type MockedGame } from "../_shared/mockGame";
import { actor, opponent, playerWithHand } from "../_shared/players";

const tokens = { tactic: 3, fleet: 3, strategy: 2 };
const act = (id: string, label: string) => ({ id, label, kind: "action" });

/** The opening action-phase menu with the given options (closing: false). */
export const turnMenu = (options: ReturnType<typeof act>[], nonce: string): MockChoice => ({
  prompt: "action phase",
  nonce,
  options,
  details: {
    kind: "turn_menu",
    closing: false,
    tokens,
    partners: [],
    strategy_cards: [{ card: "pok8imperial", used: false, option: "strategic" }],
  },
});

export const STRATEGIC = act("strategic", "take your strategic action");
export const TACTICAL = act("tactical", "take a tactical action");
export const PASS = act("pass", "pass");

export const loneStrategic = (nonce: string) => turnMenu([STRATEGIC], nonce);

export const activation = (ids: string[], nonce: string): MockChoice => ({
  prompt: "activate a system",
  nonce,
  context: { subtype: "activate_system" },
  options: ids.map((id) => ({ id, label: `activate ${id}`, kind: "activate" })),
});

export interface LoneGame {
  game: MockedGame;
  /** Everything the page sent as `submit_choice`. */
  submissions: ClientMessage[];
  /** The server moves the game on: history cursor and (optionally) the next decision. */
  advance: (cursor: number, choice: MockChoice | null, redoCount?: number) => void;
}

/** Opens the mocked game with a quiet turn and records the page's submissions. */
export async function openLoneGame(page: Page, options: { spectator?: boolean } = {}): Promise<LoneGame> {
  const game = await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    choice: null,
    spectator: options.spectator,
  });
  await page.getByTestId("ti4-board-svg").waitFor();
  const submissions: ClientMessage[] = [];
  game.socket.onMessage((data) => {
    const message = JSON.parse(String(data)) as ClientMessage;
    if (message.type === "submit_choice") submissions.push(message);
  });
  const { events: _events, ...base } = game.snapshot;
  let version = game.snapshot.game_version;
  const advance = (cursor: number, choice: MockChoice | null, redoCount = 0) => {
    version += 1;
    game.send({
      ...base,
      type: "state_update",
      protocol_version: PROTOCOL_VERSION,
      game_id: GAME_ID,
      game_version: version,
      history: { cursor, redo_count: redoCount },
      pending_choice: choice
        ? {
            nonce: choice.nonce ?? "lone-nonce",
            choice: {
              player: choice.player ?? actor,
              prompt: choice.prompt,
              context: (choice.context ?? { subtype: "decision" }) as never,
              options: choice.options,
              ...(choice.details ? { details: choice.details } : {}),
            },
          }
        : null,
    });
  };
  return { game, submissions, advance };
}
