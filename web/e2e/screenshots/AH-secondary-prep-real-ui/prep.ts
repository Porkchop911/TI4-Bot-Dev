import type { Page } from "@playwright/test";
import type { BoardView, GameEvent, PlayerView } from "../../../src/protocol/types";
import { galleryBoard } from "../../../src/dev/galleryBoard";
import { openMockedGame, type MockedGame } from "../_shared/mockGame";
import { playerWithHand, opponent, actor } from "../_shared/players";

/** The viewer's seat (a follower) and the seat resolving the strategy card. */
export const PRIMARY = opponent.id;

export interface PrepScene {
  /** Card the primary seat plays, by printed name and id. */
  name: string;
  card: string;
  /** Viewer's players, with the primary holding the card (and a second one, so the log names it). */
  viewer?: Partial<PlayerView>;
  /** Board; the gallery board by default (the viewer controls Jord, Exhausted World, Wellon, Tarmann). */
  board?: BoardView;
}

/** The gallery board with some of the viewer's planets exhausted, so Diplomacy has something to ready. */
export const boardWithExhausted = (...planets: string[]): BoardView => {
  const board: BoardView = JSON.parse(JSON.stringify(galleryBoard));
  for (const system of Object.values(board.systems))
    for (const planet of Object.values(system.planets))
      if (planets.includes(planet.planet_id)) planet.exhausted = true;
  return board;
};

/** Public log: the primary's strategic action selection, as the server publishes it. */
export const playedEvent = (name: string): GameEvent =>
  ({
    id: "prep-event-1",
    timestamp: "10:20",
    version: 41,
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    decision_count: 41,
    round: 2,
    phase: "action",
    actor: PRIMARY,
    action_id: "action_21",
    action_type: "strategic",
    action_actor: PRIMARY,
    stage: "action selection",
    detail: `${PRIMARY} played ${name}`,
  }) as GameEvent;

/**
 * A mocked game where the primary seat is resolving a strategy card and the viewer waits. History
 * starts at a known cursor so a later `push` is real forward progress.
 */
export async function openWaiting(page: Page, scene: PrepScene): Promise<MockedGame> {
  const game = await openMockedGame(page, {
    players: [
      playerWithHand({ strategy_cards: ["pok8imperial"], strategic_tokens: 2, ...scene.viewer }),
      { ...opponent, strategy_cards: [scene.card, scene.card === "pok1leadership" ? "pok5trade" : "pok1leadership"] },
    ],
    board: scene.board,
    events: [playedEvent(scene.name)],
    view: { active_player: PRIMARY },
    turnStatus: { kind: "waiting_for_decision", seat: PRIMARY, phase: "action", round: 2, stage: "Waiting for player" },
  });
  // Establish a history position the arrival can move forward from.
  push(game, { history: { cursor: 40, redo_count: 0, generation: 0 }, version: 41 });
  await page.waitForTimeout(150);
  return game;
}

/** Leaves preparation mode as a player would: save the plan. */
export async function save(page: Page) {
  await page.getByTestId("prep-save").click();
}

type PendingOptions = { id: string; kind?: string; label: string; payload?: Record<string, unknown> }[];

/** A state update that moves the game forward; with `choice` it is the viewer's own question. */
export function push(
  game: MockedGame,
  update: {
    version: number;
    history: { cursor: number; redo_count: number; generation: number };
    choice?: { prompt: string; subtype: string; options: PendingOptions; details?: Record<string, unknown> } | null;
  },
) {
  const { events: _events, ...base } = game.snapshot;
  game.send({
    ...base,
    type: "state_update",
    game_version: update.version,
    history: update.history,
    turn_status: update.choice
      ? { kind: "waiting_for_decision", seat: actor, phase: "action", round: 2, stage: update.choice.subtype }
      : base.turn_status,
    pending_choice: update.choice
      ? {
          nonce: `prep-nonce-${update.version}`,
          choice: {
            player: actor,
            prompt: update.choice.prompt,
            context: { subtype: update.choice.subtype },
            options: update.choice.options,
            ...(update.choice.details ? { details: update.choice.details } : {}),
          },
        }
      : null,
  });
}

/** The follower's secondary question for a card, as the engine sends it. */
export const secondaryQuestion = (card: string, prompt: string, yesLabel: string, tokensLeft = 2) => ({
  prompt,
  subtype: "strategy_secondary",
  options: [
    { id: "no", kind: "strategy", label: "decline" },
    { id: "yes", kind: "strategy", label: yesLabel },
  ],
  details: { kind: "strategy_secondary", card, played_by: PRIMARY, tokens_left: tokensLeft, costs_token: true },
});
