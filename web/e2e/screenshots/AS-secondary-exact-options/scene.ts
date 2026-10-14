import type { Page } from "@playwright/test";
import {
  PROTOCOL_VERSION,
  type ClientMessage,
  type EngineChoice,
  type SecondaryPreviewBody,
} from "../../../src/protocol/types";
import { GAME_ID, openMockedGame, type MockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand, actor } from "../_shared/players";
import { playedEvent, PRIMARY } from "../AH-secondary-prep-real-ui/prep";
import {
  enginePayment,
  engineProduction,
  engineResearch,
  engineWindow,
} from "../../../src/test/secondaryPrepFixtures";

/** Which side of the change a run is: `before` is taken on the commit without the exact preview. */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const WINDOW_PROMPTS: Record<string, string> = {
  pok7technology: "spend a strategy token and 4 resources to research",
  pok6warfare: "spend a strategy token to produce at home",
  pok4construction: "spend a strategy token to build a structure",
};

/** Tier-0 and tier-1 technologies: what the engine lists for a seat whose planet skips open the second row. */
export const ENGINE_TECHS = ["amd", "nm", "st", "ps", "det", "pa", "sdn", "aida", "gd", "dxa", "gls", "md", "sr", "bs", "pi", "sar"];

/** The home production's build list: capacity 3, 5 resources. */
export const BUILDS = [
  { unit: "infantry", cost: 1, count: 2 },
  { unit: "fighter", cost: 1, count: 2 },
  { unit: "destroyer", cost: 1, count: 1 },
  { unit: "cruiser", cost: 2, count: 1 },
  { unit: "carrier", cost: 3, count: 1 },
  { unit: "dreadnought", cost: 4, count: 1 },
];

export type PreviewScript = (answers: string[]) => SecondaryPreviewBody | null;

/** A script answering like the engine for Technology: the window, then the research list with its payment. */
export const technologyScript: PreviewScript = (answers) => {
  if (answers.length === 0)
    return { status: "question", choice: engineWindow("pok7technology", WINDOW_PROMPTS.pok7technology), step: 0 };
  if (answers.length === 1)
    return {
      status: "question",
      choice: engineResearch(ENGINE_TECHS),
      step: 1,
      payment: { cost: 4, planets: ["jord"], trade_goods: 0, worth: 4 },
    };
  return { status: "complete" };
};

/** Warfare: the window, the build list, and the first build's payment question. */
export const warfareScript: PreviewScript = (answers) => {
  if (answers.length === 0)
    return { status: "question", choice: engineWindow("pok6warfare", WINDOW_PROMPTS.pok6warfare), step: 0 };
  if (answers.length === 1) return { status: "question", choice: engineProduction(BUILDS), step: 1 };
  if (answers.length === 2) return { status: "question", choice: enginePayment(1), step: 2 };
  return { status: "complete" };
};

export interface SceneOptions {
  name: string;
  card: string;
  /** What the server's preview answers; `null` plays an older server that does not know the message. */
  script: PreviewScript | null;
  viewer?: Parameters<typeof playerWithHand>[0];
}

/** The waiting game of AH (the primary resolves a card, the viewer waits) with a server that answers previews. */
export async function openScene(page: Page, scene: SceneOptions): Promise<MockedGame> {
  const game = await openMockedGame(page, {
    players: [
      playerWithHand({ strategy_cards: ["pok8imperial"], strategic_tokens: 2, trade_goods: 5, ...scene.viewer }),
      { ...opponent, strategy_cards: [scene.card, "pok1leadership"] },
    ],
    events: [playedEvent(scene.name)],
    view: { active_player: PRIMARY },
    turnStatus: { kind: "waiting_for_decision", seat: PRIMARY, phase: "action", round: 2, stage: "Waiting for player" },
    onClientMessage: (message: ClientMessage, reply) => {
      if (message.type !== "preview_secondary") return;
      if (!scene.script) {
        // What a server built before the message answers.
        reply({
          type: "error",
          protocol_version: PROTOCOL_VERSION,
          kind: "malformed_message",
          message: "unknown variant `preview_secondary`, expected one of `subscribe`, `submit_choice`",
        });
        return;
      }
      const body = scene.script(message.answers);
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
      stage: String(choice.context?.subtype ?? "strategy_secondary"),
    },
    pending_choice: { nonce: `as-nonce-${version}`, choice: { ...choice, player: actor } },
  });
}

/** The follower's window question, as the engine sends it when its turn comes. */
export const windowChoice = (card: string): EngineChoice => ({
  ...engineWindow(card, WINDOWS_PROMPT(card)),
  player: actor,
});
const WINDOWS_PROMPT = (card: string) => WINDOW_PROMPTS[card] ?? "spend a strategy token";
