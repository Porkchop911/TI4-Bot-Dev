import type { Page } from "@playwright/test";
import { openMockedGame } from "../_shared/mockGame";
import { opponent, playerWithHand } from "../_shared/players";
import type { CardScene } from "./cards";

/** `before` shots are taken with the same capture on the base commit (TI4_SHOT_PREFIX=before). */
export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const SIZES = [
  { label: "desktop", width: 1440, height: 900 },
  { label: "390x844", width: 390, height: 844 },
] as const;

export const pass = { id: "decline", label: "Pass", kind: "decline" };

/** The outer window offer for one card, shaped like Resolver::pick_with_context. */
export const offerId = (scene: CardScene) => `reaction:hacan:${scene.event}:${scene.relation}`;

/** The decision the engine asks: the trigger plus (new) the preview of the card. */
export function reactionChoice(scene: CardScene) {
  const id = offerId(scene);
  return {
    prompt: `${scene.relation} ${scene.event}`,
    context: {
      subtype: `reaction_${scene.relation}_${scene.event}`,
      optional: true,
      source: { Reaction: scene.event },
      trigger: scene.trigger,
      previews: [{ option: id, card: scene.alias, ...scene.preview }],
    },
    options: [
      { id, label: `Play ${scene.name}`, kind: "ability", payload: { card: scene.alias, card_name: scene.name, event: scene.event } },
      pass,
    ],
  };
}

export async function openScene(page: Page, scene: CardScene, size: (typeof SIZES)[number]) {
  await page.setViewportSize({ width: size.width, height: size.height });
  await openMockedGame(page, {
    players: [playerWithHand({ held_action_cards: [scene.alias], action_cards_count: 1 }), opponent],
    choice: reactionChoice(scene),
  });
  await page.getByTestId("reaction-status-bar").waitFor();
  await page.waitForTimeout(300);
}
