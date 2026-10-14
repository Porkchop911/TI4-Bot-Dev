import { expect, type Page } from "@playwright/test";
import {
  PROTOCOL_VERSION,
  type ClientMessage,
  type EngineChoice,
  type SecondaryPreviewBody,
} from "../../../src/protocol/types";
import { GAME_ID, openMockedGame, type MockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand, actor } from "../_shared/players";
import { playedEvent, PRIMARY } from "../AH-secondary-prep-real-ui/prep";
import { leadershipBoard } from "../V-leadership-map-payment/leadership";
import { engineWindow } from "../../../src/test/secondaryPrepFixtures";

/** Which side of the change a run is: `before` is taken on the commit without the exact payment. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

const CARD = "pok1leadership";

export const PLANETS = [
  { id: "jord", worth: 2 },
  { id: "lodor", worth: 3 },
  { id: "quann", worth: 1 },
];

/** The Leadership window as the engine sends it: the purchase facts ride on the question. */
export function leadershipWindow(planets = PLANETS, tradeGoods = 2): EngineChoice {
  const worth = planets.reduce((sum, planet) => sum + planet.worth, 0) + tradeGoods;
  return {
    ...engineWindow(CARD, "spend 3 influence for a command token", {
      mode: "buy",
      costs_token: false,
      pools: { tactic: 3, fleet: 4, strategic: 2 },
      tokens_to_place: 0,
      reinforcements: 8,
      purchase: {
        cost: 3,
        influence_available: worth,
        max: Math.floor(worth / 3),
        trade_goods: tradeGoods,
        trade_good_worth: 1,
        planets,
      },
    }),
    context: { subtype: "buy_token_with_influence" } as EngineChoice["context"],
  };
}

export type PreviewScript = (answers: string[]) => SecondaryPreviewBody | null;

/** The engine accepts any scripted purchase (its payment, pool and "again?" all fit). */
export const acceptingScript: PreviewScript = (answers) =>
  answers.length === 0
    ? { status: "question", choice: leadershipWindow(), step: 0 }
    : { status: "complete" };

/** The engine does not offer the scripted payment: it names the question and the answer. */
export const rejectingScript: PreviewScript = (answers) =>
  answers.length === 0
    ? { status: "question", choice: leadershipWindow(), step: 0 }
    : {
        status: "rejected",
        at: 1,
        answer: answers[1] ?? "",
        choice: {
          player: actor,
          prompt: "pay 3 more influence",
          options: [{ id: "exhaust|lodor", kind: "pay", label: "exhaust lodor for 3 influence" }],
          context: { subtype: "pay_influence" } as EngineChoice["context"],
        },
      };

/** The waiting game of AH (the primary resolves Leadership, the viewer waits) with a server that answers previews. */
export async function openScene(page: Page, script: PreviewScript): Promise<MockedGame> {
  const game = await openMockedGame(page, {
    players: [
      playerWithHand({ strategy_cards: ["pok8imperial"], strategic_tokens: 2, trade_goods: 2 }),
      { ...opponent, strategy_cards: [CARD, "pok5trade"] },
    ],
    board: leadershipBoard,
    events: [playedEvent("Leadership")],
    view: { active_player: PRIMARY },
    turnStatus: { kind: "waiting_for_decision", seat: PRIMARY, phase: "action", round: 2, stage: "Waiting for player" },
    onClientMessage: (message: ClientMessage, reply) => {
      if (message.type !== "preview_secondary") return;
      const body = script(message.answers);
      reply({
        type: "secondary_preview",
        protocol_version: PROTOCOL_VERSION,
        game_id: GAME_ID,
        request_id: message.request_id,
        as_of_version: 41,
        as_of_decisions: 41,
        card: message.card,
        outcome: body
          ? { result: "preview", preview: body }
          : { result: "refused", reason: "no_strategic_action", detail: "no strategic action is in progress" },
      });
    },
  });
  game.send({
    ...(({ events: _events, ...base }) => base)(game.snapshot),
    type: "state_update",
    game_version: 41,
    history: { cursor: 40, redo_count: 0, generation: 0 },
  });
  await page.waitForTimeout(150);
  return game;
}

/** A state update that moves the game forward and puts `choice` to the viewer (full engine shape). */
export function pushChoice(game: MockedGame, version: number, choice: EngineChoice) {
  const { events: _events, ...base } = game.snapshot;
  game.send({
    ...base,
    type: "state_update",
    game_version: version,
    history: { cursor: version - 1, redo_count: 0, generation: 0 },
    turn_status: {
      kind: "waiting_for_decision",
      seat: actor,
      phase: "action",
      round: 2,
      stage: String(choice.context?.subtype ?? "buy_token_with_influence"),
    },
    pending_choice: { nonce: `au-nonce-${version}`, choice: { ...choice, player: actor } },
  });
}

/** Opens preparation, buys one token into the fleet pool and pays on the map with Jord + Quann (2 + 1) instead of Auto-pay's Lodor. */
export async function chooseThePayment(page: Page) {
  await page.getByTestId("secondary-prep-chip").click();
  await page.getByTestId("command-token-panel").waitFor();
  await page.getByTestId("token-buy-plus").click();
  await page.getByTestId("token-plus-fleet").click();
  await page.getByTestId("token-pay-on-map").click();
  await page.getByTestId("token-payment-bar").waitFor();
  // Auto-pay's pick (Lodor) off; the player's own choice on.
  for (const planet of ["lodor", "jord", "quann"]) await page.getByTestId(`planet-${planet}`).click();
  await expect(page.getByTestId("token-bar-remaining")).toContainText("Covered");
}

/** Chooses the payment and saves the plan with the bar's confirm. */
export async function prepareAndSave(page: Page) {
  await chooseThePayment(page);
  await page.getByTestId("token-bar-confirm").click();
  await page.getByTestId("secondary-prep-chip").waitFor();
}
